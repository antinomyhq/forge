use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::{
    AgentRepository, DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra, TemplateEngine,
};
use forge_config::ForgeConfig;
use forge_domain::{ModelId, ProviderId, Template};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::Serialize;

use crate::agent_definition::AgentDefinition;

/// Infrastructure implementation for loading agent definitions from multiple
/// sources:
/// 1. Built-in agents (embedded in the application)
/// 2. Global custom agents (from ~/.forge/agents/ directory)
/// 3. Project-local agents (from .forge/agents/ directory in current working
///    directory)
///
/// ## Agent Precedence
/// When agents have duplicate IDs across different sources, the precedence
/// order is: **CWD (project-local) > Global custom > Built-in**
///
/// This means project-local agents can override global agents, and both can
/// override built-in agents.
///
/// ## Directory Resolution
/// - **Built-in agents**: Embedded in application binary
/// - **Global agents**: `~/forge/agents/*.md`
/// - **CWD agents**: `./.forge/agents/*.md` (relative to current working
///   directory)
///
/// Missing directories are handled gracefully and don't prevent loading from
/// other sources.
pub struct ForgeAgentRepository<I> {
    infra: Arc<I>,
}

impl<I> ForgeAgentRepository<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
}

impl<I: FileInfoInfra + EnvironmentInfra<Config = ForgeConfig> + DirectoryReaderInfra>
    ForgeAgentRepository<I>
{
    /// Load all agent definitions from all available sources with conflict
    /// resolution.
    async fn load_agents(&self) -> anyhow::Result<Vec<AgentDefinition>> {
        let config = self.infra.get_config()?;
        self.load_all_agents(&config).await
    }

    /// Load all agent definitions from all available sources
    async fn load_all_agents(&self, config: &ForgeConfig) -> anyhow::Result<Vec<AgentDefinition>> {
        // Load built-in agents (no path - will display as "BUILT IN")
        let mut agents = self.init_default(config).await?;

        // Load custom agents from global directory
        let dir = self.infra.get_environment().agent_path();
        let custom_agents = self.init_agent_dir(&dir, config).await?;
        agents.extend(custom_agents);

        // Load custom agents from CWD
        let dir = self.infra.get_environment().agent_cwd_path();
        let cwd_agents = self.init_agent_dir(&dir, config).await?;
        agents.extend(cwd_agents);

        // Handle agent ID conflicts by keeping the last occurrence
        // This gives precedence order: CWD > Global Custom > Built-in
        Ok(resolve_agent_conflicts(agents))
    }

    async fn init_default(&self, config: &ForgeConfig) -> anyhow::Result<Vec<AgentDefinition>> {
        parse_agent_iter(
            [
                ("forge", include_str!("agents/forge.md")),
                ("muse", include_str!("agents/muse.md")),
                ("sage", include_str!("agents/sage.md")),
            ]
            .into_iter()
            .map(|(name, content)| (name.to_string(), content.to_string())),
            config,
        )
    }

    async fn init_agent_dir(
        &self,
        dir: &std::path::Path,
        config: &ForgeConfig,
    ) -> anyhow::Result<Vec<AgentDefinition>> {
        if !self.infra.exists(dir).await? {
            return Ok(vec![]);
        }

        // Use DirectoryReaderInfra to read all .md files in parallel
        let files = self
            .infra
            .read_directory_files(dir, Some("*.md"))
            .await
            .with_context(|| format!("Failed to read agents from: {}", dir.display()))?;

        let mut agents = Vec::new();
        for (path, content) in files {
            let mut agent = parse_agent_file(&content, config)
                .with_context(|| format!("Failed to parse agent: {}", path.display()))?;

            // Store the file path
            agent.path = Some(path.display().to_string());
            agents.push(agent);
        }

        Ok(agents)
    }
}

/// Implementation function for resolving agent ID conflicts by keeping the last
/// occurrence. This implements the precedence order: CWD Custom > Global Custom
/// > Built-in
fn resolve_agent_conflicts(agents: Vec<AgentDefinition>) -> Vec<AgentDefinition> {
    use std::collections::HashMap;

    // Use HashMap to deduplicate by agent ID, keeping the last occurrence
    let mut agent_map: HashMap<String, AgentDefinition> = HashMap::new();

    for agent in agents {
        agent_map.insert(agent.id.to_string(), agent);
    }

    // Convert back to vector (order is not guaranteed but doesn't matter for the
    // service)
    agent_map.into_values().collect()
}

fn parse_agent_iter<I, Path: AsRef<str>, Content: AsRef<str>>(
    contents: I,
    config: &ForgeConfig,
) -> anyhow::Result<Vec<AgentDefinition>>
where
    I: Iterator<Item = (Path, Content)>,
{
    let mut agents = vec![];

    for (name, content) in contents {
        let agent = parse_agent_file(content.as_ref(), config)
            .with_context(|| format!("Failed to parse agent: {}", name.as_ref()))?;

        agents.push(agent);
    }

    Ok(agents)
}

#[derive(Serialize)]
struct AgentTemplateContext<'a> {
    config: &'a ForgeConfig,
}

fn render_tools_frontmatter_block(content: &str, config: &ForgeConfig) -> Result<String> {
    let Some((newline, content)) = content
        .strip_prefix("---\r\n")
        .map(|content| ("\r\n", content))
        .or_else(|| content.strip_prefix("---\n").map(|content| ("\n", content)))
    else {
        return Ok(content.to_string());
    };

    let delimiter = format!("{newline}---{newline}");
    let (frontmatter, body) = content
        .split_once(&delimiter)
        .context("Failed to find end of agent frontmatter")?;

    let rendered_frontmatter = render_tools_block(frontmatter, config)?;

    Ok(format!(
        "---{newline}{rendered_frontmatter}{delimiter}{body}"
    ))
}

fn render_tools_block(frontmatter: &str, config: &ForgeConfig) -> Result<String> {
    let lines = frontmatter
        .split_inclusive('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let Some(start) = lines.iter().position(|line| line.trim_end() == "tools:") else {
        return Ok(frontmatter.to_string());
    };

    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            let trimmed = line.trim_end();

            !trimmed.is_empty()
                && !line.starts_with([' ', '\t'])
                && !trimmed.starts_with("#")
                && !trimmed.starts_with("---")
        })
        .map(|index| start + 1 + index)
        .unwrap_or(lines.len());

    let rendered_tools_block = TemplateEngine::default().render_template(
        Template::new(lines[start..end].join("")),
        &AgentTemplateContext { config },
    )?;

    Ok(format!(
        "{}{}{}",
        lines[..start].join(""),
        rendered_tools_block,
        lines[end..].join("")
    ))
}

/// Parse raw content into an AgentDefinition with YAML frontmatter
fn parse_agent_file(content: &str, config: &ForgeConfig) -> Result<AgentDefinition> {
    let rendered_content = render_tools_frontmatter_block(content, config)?;

    // Parse the frontmatter using gray_matter with type-safe deserialization
    let gray_matter = Matter::<YAML>::new();
    let result = gray_matter.parse::<AgentDefinition>(&rendered_content)?;

    // Extract the frontmatter
    let agent = result
        .data
        .context("Empty system prompt content")?
        .system_prompt(Template::new(result.content));

    Ok(agent)
}

#[async_trait::async_trait]
impl<F: FileInfoInfra + EnvironmentInfra<Config = ForgeConfig> + DirectoryReaderInfra>
    AgentRepository for ForgeAgentRepository<F>
{
    async fn get_agents(&self) -> anyhow::Result<Vec<forge_domain::Agent>> {
        let agent_defs = self.load_agents().await?;

        let session = self
            .infra
            .get_config()?
            .session
            .ok_or(forge_domain::Error::NoDefaultSession)?;

        Ok(agent_defs
            .into_iter()
            .map(|def| {
                def.into_agent(
                    ProviderId::from(session.provider_id.clone()),
                    ModelId::from(session.model_id.clone()),
                )
            })
            .collect())
    }

    async fn get_agent_infos(&self) -> anyhow::Result<Vec<forge_domain::AgentInfo>> {
        let agent_defs = self.load_agents().await?;
        Ok(agent_defs
            .into_iter()
            .map(|def| forge_domain::AgentInfo {
                id: def.id,
                title: def.title,
                description: def.description,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::AgentId;
    use insta::{assert_snapshot, assert_yaml_snapshot};
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_parse_basic_agent() {
        let content = forge_test_kit::fixture!("/src/fixtures/agents/basic.md").await;

        let actual = parse_agent_file(&content, &ForgeConfig::default()).unwrap();

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
        let content = forge_test_kit::fixture!("/src/fixtures/agents/advanced.md").await;

        let actual = parse_agent_file(&content, &ForgeConfig::default()).unwrap();

        assert_eq!(actual.id.as_str(), "test-advanced");
        assert_eq!(actual.title.as_ref().unwrap(), "Advanced Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "An advanced test agent with full configuration"
        );
    }

    #[test]
    fn test_parse_agent_file_renders_conditional_frontmatter_when_subagents_enabled() {
        let fixture = r#"---
id: "test"
tools:
  - read
  {{#if config.enable_subagents}}
  - task
  {{else}}
  - sage
  {{/if}}
---
Body keeps {{tool_names.read}} untouched.
"#;
        let config = ForgeConfig { enable_subagents: true, ..Default::default() };

        let actual = parse_agent_file(fixture, &config).unwrap();

        assert_eq!(actual.id, AgentId::new("test"));
        assert_eq!(
            actual.system_prompt.unwrap().template,
            "Body keeps {{tool_names.read}} untouched."
        );
        assert_yaml_snapshot!("parse_agent_file_subagents_enabled_tools", actual.tools);
    }

    #[test]
    fn test_parse_agent_file_renders_conditional_frontmatter_when_subagents_disabled() {
        let fixture = r#"---
id: "test"
tools:
  - read
  {{#if config.enable_subagents}}
  - task
  {{else}}
  - sage
  {{/if}}
---
Body keeps {{tool_names.read}} untouched.
"#;
        let config = ForgeConfig { enable_subagents: false, ..Default::default() };

        let actual = parse_agent_file(fixture, &config).unwrap();

        assert_eq!(actual.id, AgentId::new("test"));
        assert_snapshot!(
            "parse_agent_file_subagents_disabled_prompt",
            actual.system_prompt.unwrap().template
        );
        assert_yaml_snapshot!("parse_agent_file_subagents_disabled_tools", actual.tools);
    }

    #[test]
    fn test_parse_agent_file_preserves_runtime_user_prompt_variables() {
        let fixture = r#"---
id: "test"
tools:
  - read
  {{#if config.enable_subagents}}
  - task
  {{else}}
  - sage
  {{/if}}
user_prompt: |-
  <{{event.name}}>{{event.value}}</{{event.name}}>
  <system_date>{{current_date}}</system_date>
---
Body keeps {{tool_names.read}} untouched.
"#;
        let config = ForgeConfig { enable_subagents: true, ..Default::default() };

        let actual = parse_agent_file(fixture, &config).unwrap();

        assert_eq!(actual.id, AgentId::new("test"));
        assert_snapshot!(
            "parse_agent_file_preserves_runtime_user_prompt_variables",
            actual.user_prompt.unwrap().template
        );
        assert_yaml_snapshot!(
            "parse_agent_file_preserves_runtime_user_prompt_variables_tools",
            actual.tools
        );
    }
}
