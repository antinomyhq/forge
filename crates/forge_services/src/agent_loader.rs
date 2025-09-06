use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{Agent, Template, ToolsDiscriminants};
use gray_matter::Matter;
use gray_matter::engine::YAML;

use crate::{
    DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra,
};

/// A service for loading agent definitions from individual files in the
/// forge/agent directory
pub struct AgentLoaderService<F> {
    infra: Arc<F>,

    // Cache is used to maintain the loaded agents
    // for this service instance.
    // So that they could live till user starts a new session.
    cache: tokio::sync::OnceCell<Vec<Agent>>,
}

impl<F> AgentLoaderService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, cache: Default::default() }
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    forge_app::AgentLoaderService for AgentLoaderService<F>
{
    /// Load all agent definitions from the forge/agent directory
    async fn get_agents(&self) -> anyhow::Result<Vec<Agent>> {
        self.cache_or_init().await
    }
}

impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    AgentLoaderService<F>
{
    /// Load all agent definitions from the forge/agent directory
    async fn cache_or_init(&self) -> anyhow::Result<Vec<Agent>> {
        self.cache.get_or_try_init(|| self.init()).await.cloned()
    }

    async fn init(&self) -> anyhow::Result<Vec<Agent>> {
        // Load built-in agents
        let mut agents = self.init_default().await?;

        // Load custom agents
        let custom_agents = self.init_custom().await?;
        agents.extend(custom_agents);

        Ok(agents)
    }

    async fn init_default(&self) -> anyhow::Result<Vec<Agent>> {
        parse_agent_iter(
            [
                ("forge", include_str!("agents/forge.md")),
                ("muse", include_str!("agents/muse.md")),
                ("prime", include_str!("agents/prime.md")),
                ("parker", include_str!("agents/parker.md")),
                ("sage", include_str!("agents/sage.md")),
            ]
            .into_iter()
            .map(|(name, content)| (name.to_string(), content.to_string())),
        )
    }

    async fn init_custom(&self) -> anyhow::Result<Vec<Agent>> {
        let agent_dir = self.infra.get_environment().agent_path();
        if !self.infra.exists(&agent_dir).await? {
            return Ok(vec![]);
        }

        // Use DirectoryReaderInfra to read all .md files in parallel
        let files = self
            .infra
            .read_directory_files(&agent_dir, Some("*.md"))
            .await
            .with_context(|| "Failed to read agent directory")?;

        parse_agent_iter(files.into_iter().map(|(path, content)| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            (name, content)
        }))
    }
}

fn parse_agent_iter<I, Path: AsRef<str>, Content: AsRef<str>>(
    contents: I,
) -> anyhow::Result<Vec<Agent>>
where
    I: Iterator<Item = (Path, Content)>,
{
    let mut agents = vec![];

    for (name, content) in contents {
        agents.push(
            parse_agent_file(content.as_ref())
                .with_context(|| format!("Failed to parse agent: {}", name.as_ref()))?,
        );
    }

    Ok(agents)
}

/// Parse raw content into an Agent with YAML frontmatter
fn parse_agent_file(content: &str) -> Result<Agent> {
    // Parse the frontmatter using gray_matter with type-safe deserialization
    let gray_matter = Matter::<YAML>::new();
    let result = gray_matter.parse::<Agent>(content)?;

    // Extract the frontmatter
    let agent = result
        .data
        .context("Empty system prompt content")?
        .system_prompt(Template::new(result.content));

    // Add attempt completion tool by default if not already present
    Ok(add_attempt_completion_tool(agent))
}

/// Adds the attempt completion tool to the agent's tools list by default
fn add_attempt_completion_tool(mut agent: Agent) -> Agent {
    let completion_tool = ToolsDiscriminants::AttemptCompletion.name();

    if let Some(tools) = agent.tools.as_mut() {
        // If agent supports tool calling and doesn't have it already
        if !tools.contains(&completion_tool) && !tools.is_empty() {
            tools.push(completion_tool);
        }
    }

    agent
}

#[cfg(test)]
mod tests {
    use forge_app::domain::ToolName;
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_parse_basic_agent() {
        let content = include_str!("fixtures/agents/basic.md");

        let actual = parse_agent_file(content).unwrap();

        assert_eq!(actual.id.as_str(), "test-basic");
        assert_eq!(actual.title.as_ref().unwrap(), "Basic Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "A simple test agent for basic functionality"
        );
        assert_eq!(
            actual.system_prompt.as_ref().unwrap().template,
            "This is a basic test agent used for testing fundamental functionality."
        );
    }

    #[tokio::test]
    async fn test_parse_advanced_agent() {
        let content = include_str!("fixtures/agents/advanced.md");

        let actual = parse_agent_file(content).unwrap();

        assert_eq!(actual.id.as_str(), "test-advanced");
        assert_eq!(actual.title.as_ref().unwrap(), "Advanced Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "An advanced test agent with full configuration"
        );
        assert_eq!(
            actual.model.as_ref().unwrap().as_str(),
            "claude-3-5-sonnet-20241022"
        );
        assert_eq!(actual.tool_supported, Some(true));
        assert!(actual.tools.is_some());
        assert_eq!(actual.temperature.as_ref().unwrap().value(), 0.7);
        assert_eq!(actual.top_p.as_ref().unwrap().value(), 0.9);
        assert_eq!(actual.max_tokens.as_ref().unwrap().value(), 2000);
        assert_eq!(actual.max_turns, Some(10));
        assert!(actual.reasoning.is_some());
    }

    #[tokio::test]
    async fn test_parse_invalid_frontmatter() {
        let content = include_str!("fixtures/agents/invalid.md");

        let result = parse_agent_file(content);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_builtin_agents() {
        // Test that all built-in agents parse correctly
        let builtin_agents = [
            ("forge", include_str!("agents/forge.md")),
            ("muse", include_str!("agents/muse.md")),
            ("prime", include_str!("agents/prime.md")),
            ("parker", include_str!("agents/parker.md")),
            ("sage", include_str!("agents/sage.md")),
        ];

        for (name, content) in builtin_agents {
            let agent = parse_agent_file(content)
                .with_context(|| format!("Failed to parse built-in agent: {}", name))
                .unwrap();

            assert_eq!(agent.id.as_str(), name);
            assert!(agent.title.is_some());
            assert!(agent.description.is_some());
            assert!(agent.system_prompt.is_some());
        }
    }
    #[test]
    fn test_add_attempt_completion_tool_with_no_tools() {
        let fixture = Agent::new("test-add-completion-no-tools")
            .title("Test Agent - No Tools")
            .description("Agent without any tools field for testing add_attempt_completion_tool")
            .system_prompt(Template::new("Agent fixture for testing add_attempt_completion_tool function with no tools field."));

        let actual = add_attempt_completion_tool(fixture.clone());
        let expected = fixture; // Should remain unchanged

        // Compare relevant fields since Agent doesn't implement PartialEq
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.tools, expected.tools);
        assert!(actual.tools.is_none());
    }

    #[test]
    fn test_add_attempt_completion_tool_with_empty_tools() {
        let fixture = Agent::new("test-add-completion-empty-tools")
            .title("Test Agent - Empty Tools")
            .description("Agent with empty tools list for testing add_attempt_completion_tool")
            .tools(Vec::<ToolName>::new())
            .system_prompt(Template::new("Agent fixture for testing add_attempt_completion_tool function with empty tools list."));

        let actual = add_attempt_completion_tool(fixture.clone());
        let expected = fixture; // Should remain unchanged

        // Compare relevant fields since Agent doesn't implement PartialEq
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.tools, expected.tools);
        assert_eq!(actual.tools.as_ref().unwrap(), &Vec::<ToolName>::new());
    }

    #[test]
    fn test_add_attempt_completion_tool_already_has_completion() {
        let fixture = Agent::new("test-add-completion-has-completion")
            .title("Test Agent - Has Completion")
            .description("Agent that already has attempt_completion for testing add_attempt_completion_tool")
            .tools(vec![
                ToolName::new("fs_read"),
                ToolName::new("attempt_completion"),
                ToolName::new("shell")
            ])
            .system_prompt(Template::new("Agent fixture for testing add_attempt_completion_tool function when attempt_completion already exists."));

        let actual = add_attempt_completion_tool(fixture.clone());
        let expected = fixture; // Should remain unchanged

        // Compare relevant fields since Agent doesn't implement PartialEq
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.tools, expected.tools);
        let tools = actual.tools.as_ref().unwrap();
        assert!(tools.contains(&ToolName::new("attempt_completion")));
        // Should not duplicate the tool
        assert_eq!(
            tools
                .iter()
                .filter(|&tool| *tool == ToolName::new("attempt_completion"))
                .count(),
            1
        );
    }

    #[test]
    fn test_add_attempt_completion_tool_should_add_completion() {
        let fixture = Agent::new("test-add-completion-needs-completion")
            .title("Test Agent - Needs Completion")
            .description("Agent with tools but missing attempt_completion for testing add_attempt_completion_tool")
            .tools(vec![
                ToolName::new("fs_read"),
                ToolName::new("fs_write"),
                ToolName::new("shell")
            ])
            .system_prompt(Template::new("Agent fixture for testing add_attempt_completion_tool function when attempt_completion needs to be added."));

        let actual = add_attempt_completion_tool(fixture.clone());

        // Create expected result manually
        let mut expected_tools = fixture.tools.as_ref().unwrap().clone();
        expected_tools.push(ToolName::new("attempt_completion"));

        // Compare relevant fields
        assert_eq!(actual.id, fixture.id);
        assert_eq!(actual.tools.as_ref().unwrap(), &expected_tools);
        let tools = actual.tools.as_ref().unwrap();
        assert!(tools.contains(&ToolName::new("attempt_completion")));
        assert_eq!(tools.len(), 4); // Original 3 + 1 added
    }
}
