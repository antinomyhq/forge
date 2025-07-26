use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{Agent, AgentId};
use forge_domain::Environment;
use forge_walker::Walker;
use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::{Deserialize, Serialize};

use crate::{FileInfoInfra, FileReaderInfra, FileWriterInfra};

/// A service for loading agent definitions from individual files in the
/// forge/agent directory
pub struct AgentLoaderService<F> {
    infra: Arc<F>,
    environment: Environment,
}

/// Represents the frontmatter structure of an agent definition file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentFrontmatter {
    #[serde(flatten)]
    agent: Agent,
}

/// Internal representation of a complete agent definition with frontmatter and
/// markdown content
#[derive(Debug, Clone)]
struct InternalAgentDefinitionFile {
    pub agent: Agent,
}

impl<F> AgentLoaderService<F> {
    pub fn new(infra: Arc<F>, environment: Environment) -> Self {
        Self { infra, environment }
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra> forge_app::AgentLoaderService
    for AgentLoaderService<F>
{
    /// Load all agent definitions from the forge/agent directory
    async fn load_agents(&self) -> anyhow::Result<Vec<Agent>> {
        let agent_dir = self.environment.agent_path();
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
                let agent_def = self.parse_agent_file(path).await?;
                agents.push(agent_def.agent)
            }
        }

        Ok(agents)
    }
}

impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra> AgentLoaderService<F> {
    /// Parse an individual agent definition file with YAML frontmatter
    async fn parse_agent_file(&self, path: PathBuf) -> Result<InternalAgentDefinitionFile> {
        let content = self
            .infra
            .read_utf8(&path)
            .await
            .with_context(|| format!("Failed to read agent file: {}", path.display()))?;

        // Parse the frontmatter using gray_matter with type-safe deserialization
        let matter = Matter::<YAML>::new();
        let result = matter
            .parse::<AgentFrontmatter>(&content)
            .with_context(|| format!("Failed to parse YAML frontmatter in {}", path.display()))?;

        // Extract the frontmatter
        let frontmatter = result
            .data
            .context(format!("No YAML frontmatter found in {}", path.display()))?;

        // Use the filename (without extension) as the agent ID if not specified
        let mut agent = frontmatter.agent;
        if agent.id == AgentId::default() {
            let filename = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid filename: {}", path.display()))?;
            agent.id = AgentId::new(filename);
        }

        Ok(InternalAgentDefinitionFile { agent })
    }
}
