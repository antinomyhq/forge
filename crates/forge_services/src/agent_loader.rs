use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{Agent, Template};
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
        let builtin_agents = [
            ("forge", include_str!("agents/forge.md")),
            ("muse", include_str!("agents/muse.md")),
            ("prime", include_str!("agents/prime.md")),
            ("parker", include_str!("agents/parker.md")),
            ("sage", include_str!("agents/sage.md")),
        ];

        let mut agents = Vec::new();
        for (name, content) in builtin_agents {
            let path_str = format!("{}.md", name);
            let path = std::path::Path::new(&path_str);
            agents.push(parse_agent_file_by_format(content, path)?);
        }

        Ok(agents)
    }

    async fn init_custom(&self) -> anyhow::Result<Vec<Agent>> {
        let agent_dir = self.infra.get_environment().agent_path();
        if !self.infra.exists(&agent_dir).await? {
            return Ok(vec![]);
        }

        // Read all supported agent file formats
        let mut all_files = Vec::new();

        // Read .md files
        if let Ok(md_files) = self
            .infra
            .read_directory_files(&agent_dir, Some("*.md"))
            .await
        {
            all_files.extend(md_files);
        }

        // Read .yaml files
        if let Ok(yaml_files) = self
            .infra
            .read_directory_files(&agent_dir, Some("*.yaml"))
            .await
        {
            all_files.extend(yaml_files);
        }

        // Read .yml files
        if let Ok(yml_files) = self
            .infra
            .read_directory_files(&agent_dir, Some("*.yml"))
            .await
        {
            all_files.extend(yml_files);
        }

        // Read .json files
        if let Ok(json_files) = self
            .infra
            .read_directory_files(&agent_dir, Some("*.json"))
            .await
        {
            all_files.extend(json_files);
        }

        if all_files.is_empty() {
            return Ok(vec![]);
        }

        let mut agents = Vec::new();
        for (path, content) in all_files {
            agents.push(parse_agent_file_by_format(&content, &path)?);
        }

        Ok(agents)
    }
}

/// Parse agent file based on its format (detected from file extension)
fn parse_agent_file_by_format(content: &str, file_path: &std::path::Path) -> Result<Agent> {
    match file_path.extension().and_then(|ext| ext.to_str()) {
        Some("md") => parse_markdown_agent_file(content),
        Some("yaml") | Some("yml") => parse_yaml_agent_file(content),
        Some("json") => parse_json_agent_file(content),
        _ => parse_markdown_agent_file(content), // Default to markdown for unknown extensions
    }
}

/// Parse raw content into an Agent with YAML frontmatter (Markdown format)
fn parse_markdown_agent_file(content: &str) -> Result<Agent> {
    // Parse the frontmatter using gray_matter with type-safe deserialization
    let gray_matter = Matter::<YAML>::new();
    let result = gray_matter.parse::<Agent>(content)?;

    // Extract the frontmatter
    let agent = result
        .data
        .context("Empty system prompt content")?
        .system_prompt(Template::new(result.content));

    Ok(agent)
}

/// Parse pure YAML agent file
fn parse_yaml_agent_file(content: &str) -> Result<Agent> {
    let agent: Agent = serde_yml::from_str(content)
        .context("Failed to parse YAML agent file")?;
    
    // Validate that system_prompt is present
    if agent.system_prompt.is_none() {
        return Err(anyhow::anyhow!("system_prompt field is required in YAML agent files"));
    }

    Ok(agent)
}

/// Parse pure JSON agent file
fn parse_json_agent_file(content: &str) -> Result<Agent> {
    let agent: Agent = serde_json::from_str(content)
        .context("Failed to parse JSON agent file")?;
    
    // Validate that system_prompt is present
    if agent.system_prompt.is_none() {
        return Err(anyhow::anyhow!("system_prompt field is required in JSON agent files"));
    }

    Ok(agent)
}


#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_parse_basic_agent() {
        let content = include_str!("fixtures/agents/basic.md");

        let actual = parse_markdown_agent_file(content).unwrap();

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

        let actual = parse_markdown_agent_file(content).unwrap();

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

        let result = parse_markdown_agent_file(content);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_basic_yaml_agent() {
        let content = include_str!("fixtures/agents/basic.yaml");

        let actual = parse_yaml_agent_file(content).unwrap();

        assert_eq!(actual.id.as_str(), "test-basic-yaml");
        assert_eq!(actual.title.as_ref().unwrap(), "Basic YAML Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "A simple test agent for basic functionality defined in YAML"
        );
        assert_eq!(
            actual.system_prompt.as_ref().unwrap().template,
            "You are a helpful assistant for basic tasks. This is a YAML-defined agent."
        );
    }

    #[tokio::test]
    async fn test_parse_basic_json_agent() {
        let content = include_str!("fixtures/agents/basic.json");

        let actual = parse_json_agent_file(content).unwrap();

        assert_eq!(actual.id.as_str(), "test-basic-json");
        assert_eq!(actual.title.as_ref().unwrap(), "Basic JSON Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "A simple test agent for basic functionality defined in JSON"
        );
        assert_eq!(
            actual.system_prompt.as_ref().unwrap().template,
            "You are a helpful assistant for basic tasks. This is a JSON-defined agent."
        );
    }

    #[tokio::test]
    async fn test_parse_advanced_yaml_agent() {
        let content = include_str!("fixtures/agents/advanced.yaml");

        let actual = parse_yaml_agent_file(content).unwrap();

        assert_eq!(actual.id.as_str(), "test-advanced-yaml");
        assert_eq!(actual.title.as_ref().unwrap(), "Advanced YAML Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "An advanced test agent with full configuration defined in YAML"
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
        assert!(
            actual
                .system_prompt
                .as_ref()
                .unwrap()
                .template
                .contains("Advanced YAML Test Agent")
        );
    }

    #[tokio::test]
    async fn test_parse_advanced_json_agent() {
        let content = include_str!("fixtures/agents/advanced.json");

        let actual = parse_json_agent_file(content).unwrap();

        assert_eq!(actual.id.as_str(), "test-advanced-json");
        assert_eq!(actual.title.as_ref().unwrap(), "Advanced JSON Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "An advanced test agent with full configuration defined in JSON"
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
        assert!(
            actual
                .system_prompt
                .as_ref()
                .unwrap()
                .template
                .contains("Advanced JSON Test Agent")
        );
    }

    #[tokio::test]
    async fn test_parse_format_detection() {
        // Test Markdown format
        let md_content = include_str!("fixtures/agents/basic.md");
        let md_path = std::path::Path::new("test.md");
        let md_agent = parse_agent_file_by_format(md_content, md_path).unwrap();
        assert_eq!(md_agent.id.as_str(), "test-basic");

        // Test YAML format
        let yaml_content = include_str!("fixtures/agents/basic.yaml");
        let yaml_path = std::path::Path::new("test.yaml");
        let yaml_agent = parse_agent_file_by_format(yaml_content, yaml_path).unwrap();
        assert_eq!(yaml_agent.id.as_str(), "test-basic-yaml");

        // Test JSON format
        let json_content = include_str!("fixtures/agents/basic.json");
        let json_path = std::path::Path::new("test.json");
        let json_agent = parse_agent_file_by_format(json_content, json_path).unwrap();
        assert_eq!(json_agent.id.as_str(), "test-basic-json");

        // Test YML format (should be treated same as YAML)
        let yml_path = std::path::Path::new("test.yml");
        let yml_agent = parse_agent_file_by_format(yaml_content, yml_path).unwrap();
        assert_eq!(yml_agent.id.as_str(), "test-basic-yaml");
    }

    #[tokio::test]
    async fn test_parse_yaml_missing_system_prompt() {
        let content = r#"
id: "test-agent"
title: "Test Agent"
description: "A test agent without system prompt"
"#;

        let result = parse_yaml_agent_file(content);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("system_prompt field is required")
        );
    }

    #[tokio::test]
    async fn test_parse_json_missing_system_prompt() {
        let content = r#"
{
  "id": "test-agent",
  "title": "Test Agent", 
  "description": "A test agent without system prompt"
}
"#;

        let result = parse_json_agent_file(content);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("system_prompt field is required")
        );
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
            let agent = parse_markdown_agent_file(content)
                .with_context(|| format!("Failed to parse built-in agent: {}", name))
                .unwrap();

            assert_eq!(agent.id.as_str(), name);
            assert!(agent.title.is_some());
            assert!(agent.description.is_some());
            assert!(agent.system_prompt.is_some());
        }
    }
}
