use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{Agent, AgentId};
use forge_walker::Walker;
use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{EnvironmentInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra};

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
    agent: Agent,
}

impl<F> AgentLoaderService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, cache: Arc::new(Default::default()) }
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra>
    forge_app::AgentLoaderService for AgentLoaderService<F>
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
        let entries = Walker::min_all()
            .cwd(agent_dir.clone())
            .get()
            .await
            .with_context(|| "Failed to read agent directory")?;

        for entry in entries {
            let path = agent_dir.join(entry.path);

            // Only process .md files
            if entry.file_name.map(|v| v.ends_with(".md")).unwrap_or(false) {
                let content =
                    self.infra.read_utf8(&path).await.with_context(|| {
                        format!("Failed to read agent file: {}", path.display())
                    })?;

                // Extract filename (without extension) as agent ID
                let filename = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string());

                agents.push(self.parse_agent_file(&content, filename).await?)
            }
        }

        *self.cache.lock().await = Some(agents.clone());

        Ok(agents)
    }
}

impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra>
    AgentLoaderService<F>
{
    /// Parse raw content into an Agent with YAML frontmatter
    async fn parse_agent_file(&self, content: &str, agent_id: Option<String>) -> Result<Agent> {
        // Parse the frontmatter using gray_matter with type-safe deserialization
        let matter = Matter::<YAML>::new();
        let result = matter
            .parse::<AgentFrontmatter>(content)
            .with_context(|| "Failed to parse YAML frontmatter")?;

        // Extract the frontmatter
        let frontmatter = result.data.context("No YAML frontmatter found")?;

        // Use the provided agent_id or keep the existing one
        let mut agent = frontmatter.agent;
        if let Some(id) = agent_id {
            agent.id = AgentId::new(&id);
        }

        Ok(agent)
    }
}
