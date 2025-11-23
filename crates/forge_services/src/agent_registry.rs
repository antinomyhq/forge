use std::sync::Arc;

use forge_app::AgentRepository;
use forge_app::domain::AgentId;
use forge_domain::{Agent, AppConfigRepository, ProviderRepository};
use tokio::sync::RwLock;

/// AgentRegistryService manages the active-agent ID and coordinates with
/// AgentRepository to provide runtime Agent instances.
///
/// The service converts AgentDefinitions from the repository into runtime
/// Agents by applying default provider and model configurations.
/// Caching is handled internally by the AgentRepository.
pub struct ForgeAgentRegistryService<R> {
    // Infrastructure dependency for loading agent definitions and config
    repository: Arc<R>,

    // In-memory storage for the active agent ID
    active_agent_id: RwLock<Option<AgentId>>,
}

impl<R> ForgeAgentRegistryService<R> {
    /// Creates a new AgentRegistryService with the given repository
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository, active_agent_id: RwLock::new(None) }
    }
}

impl<R: AgentRepository + AppConfigRepository + ProviderRepository> ForgeAgentRegistryService<R> {
    /// Converts agent definitions from repository to runtime agents
    /// by applying default provider and model configurations
    async fn get_runtime_agents(&self) -> anyhow::Result<Vec<Agent>> {
        // Load agent definitions from repository (cached internally)
        let agent_defs = self.repository.get_agents().await?;

        // Get default provider and model from app config
        let app_config = self.repository.get_app_config().await?;
        let default_provider_id = app_config
            .provider
            .ok_or_else(|| anyhow::anyhow!("No default provider configured"))?;
        let default_provider = self.repository.get_provider(default_provider_id).await?;
        let default_model = app_config
            .model
            .get(&default_provider.id)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No default model configured for provider {}",
                    default_provider.id
                )
            })?;

        // Convert definitions to runtime agents
        let agents = agent_defs
            .into_iter()
            .map(|def| Agent::from_agent_def(def, default_provider.id, default_model.clone()))
            .collect();

        Ok(agents)
    }
}

#[async_trait::async_trait]
impl<R: AgentRepository + AppConfigRepository + ProviderRepository> forge_app::AgentRegistry
    for ForgeAgentRegistryService<R>
{
    async fn get_active_agent_id(&self) -> anyhow::Result<Option<AgentId>> {
        let agent_id = self.active_agent_id.read().await;
        Ok(agent_id.clone())
    }

    async fn set_active_agent_id(&self, agent_id: AgentId) -> anyhow::Result<()> {
        let mut active_agent = self.active_agent_id.write().await;
        *active_agent = Some(agent_id);
        Ok(())
    }

    async fn get_agents(&self) -> anyhow::Result<Vec<Agent>> {
        self.get_runtime_agents().await
    }

    async fn get_agent(&self, agent_id: &AgentId) -> anyhow::Result<Option<Agent>> {
        let agents = self.get_runtime_agents().await?;
        Ok(agents.into_iter().find(|a| a.id == *agent_id))
    }

    async fn get_agent_definitions(&self) -> anyhow::Result<Vec<forge_domain::AgentDefinition>> {
        self.repository.get_agents().await
    }
}
