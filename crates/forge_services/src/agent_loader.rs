use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{Agent, AgentId};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

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
    cache: Arc<Mutex<Option<Vec<Agent>>>>,
}

/// Represents the frontmatter structure of an agent definition file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentFrontmatter {
    #[serde(flatten)]
    agent_fields: serde_json::Value,
}

impl<F> AgentLoaderService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, cache: Arc::new(Default::default()) }
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    forge_app::AgentLoaderService for AgentLoaderService<F>
{
    /// Load all agent definitions from the forge/agent directory
    async fn load_agents(&self) -> anyhow::Result<Vec<Agent>> {
        self.load_agents().await
    }
}

impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    AgentLoaderService<F>
{
    /// Load all agent definitions from the forge/agent directory
    async fn load_agents(&self) -> anyhow::Result<Vec<Agent>> {
        if let Some(agents) = self.cache.lock().await.as_ref() {
            return Ok(agents.clone());
        }
        let agent_dir = self.infra.get_environment().agent_path();
        if !self.infra.exists(&agent_dir).await? {
            return Ok(vec![]);
        }

        let mut agents = vec![];

        // Use DirectoryReaderInfra to read all .md files in parallel
        let files = self
            .infra
            .read_directory_files(&agent_dir, Some("*.md"))
            .await
            .with_context(|| "Failed to read agent directory")?;

        for (path, content) in files {
            // Extract filename (without extension) as agent ID
            let filename = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());

            agents.push(parse_agent_file(&content, filename).await?)
        }

        *self.cache.lock().await = Some(agents.clone());

        Ok(agents)
    }
}

/// Parse raw content into an Agent with YAML frontmatter
async fn parse_agent_file(content: &str, agent_id: Option<String>) -> Result<Agent> {
    // Parse the frontmatter using gray_matter with type-safe deserialization
    let matter = Matter::<YAML>::new();
    let result = matter
        .parse::<AgentFrontmatter>(content)
        .with_context(|| "Failed to parse YAML frontmatter")?;

    // Extract the frontmatter
    let frontmatter = result.data.context("No YAML frontmatter found")?;

    // Create agent from frontmatter fields with a temporary ID
    let mut agent_value = frontmatter.agent_fields;
    // Add a temporary ID to satisfy the Agent struct requirement
    agent_value["id"] = serde_json::Value::String("temp".to_string());

    let mut agent: Agent =
        serde_json::from_value(agent_value).context("Failed to parse agent from frontmatter")?;

    // Use the provided agent_id, frontmatter id, or keep the existing one
    if let Some(id) = agent_id {
        agent.id = AgentId::new(&id);
    } else if let Some(id) = frontmatter.id {
        agent.id = AgentId::new(&id);
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

        let actual = parse_agent_file(content, None).await.unwrap();

        assert_eq!(actual.id.as_str(), "test-basic");
        assert_eq!(actual.title.as_ref().unwrap(), "Basic Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "A simple test agent for basic functionality"
        );
        assert!(actual.system_prompt.is_some());
    }

    #[tokio::test]
    async fn test_parse_advanced_agent() {
        let content = include_str!("fixtures/agents/advanced.md");

        let actual = parse_agent_file(content, None).await.unwrap();

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
    async fn test_parse_agent_with_filename_id_override() {
        let content = include_str!("fixtures/agents/no_id.md");

        let actual = parse_agent_file(content, Some("custom-id".to_string()))
            .await
            .unwrap();

        assert_eq!(actual.id.as_str(), "custom-id");
        assert_eq!(actual.title.as_ref().unwrap(), "No ID Agent");
    }

    #[tokio::test]
    async fn test_parse_invalid_frontmatter() {
        let content = include_str!("fixtures/agents/invalid.md");

        let result = parse_agent_file(content, None).await;
        assert!(result.is_err());
    }
}
