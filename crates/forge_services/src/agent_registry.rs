use std::sync::Arc;

use dashmap::DashMap;
use forge_app::domain::AgentId;
use forge_app::{Agent, AgentRepository};
use forge_domain::{AppConfigRepository, ProviderRepository};
use tokio::sync::OnceCell;

/// AgentRegistryService manages the active-agent ID and a registry of runtime
/// Agents in-memory. It lazily loads agents from AgentRepository on first
/// access.
pub struct ForgeAgentRegistryService<R> {
    // Infrastructure dependency for loading agent definitions
    repository: Arc<R>,

    // In-memory storage for agents keyed by AgentId string
    // Lazily initialized on first access
    agents: OnceCell<DashMap<String, Agent>>,

    // In-memory storage for the active agent ID
    active_agent_id: tokio::sync::RwLock<Option<AgentId>>,
}

impl<R> ForgeAgentRegistryService<R> {
    /// Creates a new AgentRegistryService with the given repository
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            agents: OnceCell::new(),
            active_agent_id: tokio::sync::RwLock::new(None),
        }
    }
}

impl<R: AgentRepository + AppConfigRepository + ProviderRepository> ForgeAgentRegistryService<R> {
    /// Lazily initializes and returns the agents map
    /// Loads agents from repository on first call, subsequent calls return
    /// cached value
    async fn ensure_agents_loaded(&self) -> anyhow::Result<&DashMap<String, Agent>> {
        self.agents
            .get_or_try_init(|| async {
                // Load agent definitions from repository
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

                // Create the agents map
                let agents_map = DashMap::new();

                // Convert definitions to runtime agents and populate map
                for def in agent_defs {
                    let agent =
                        Agent::from_agent_def(def, default_provider.id, default_model.clone());
                    agents_map.insert(agent.id.as_str().to_string(), agent);
                }

                Ok(agents_map)
            })
            .await
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
        let agents = self.ensure_agents_loaded().await?;
        Ok(agents.iter().map(|entry| entry.value().clone()).collect())
    }

    async fn get_agent(&self, agent_id: &AgentId) -> anyhow::Result<Option<Agent>> {
        let agents = self.ensure_agents_loaded().await?;
        Ok(agents.get(agent_id.as_str()).map(|v| v.value().clone()))
    }
}
