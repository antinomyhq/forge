use std::sync::Arc;

use anyhow::Result;
use forge_domain::{Agent, AgentDefinition, AgentId};

use crate::Services;
use crate::services::{AgentLoader, AgentRegistry, AppConfigService};

/// Orchestrates agent operations including loading definitions, converting to
/// runtime agents, and managing the agent registry.
///
/// This struct provides a high-level API for working with agents by:
/// - Loading agent definitions from the AgentLoader
/// - Converting definitions to runtime Agents with default provider/model
/// - Populating and querying the AgentRegistry
pub struct AgentOrchestrator<S> {
    services: Arc<S>,
}

impl<S: Services> AgentOrchestrator<S> {
    /// Creates a new AgentOrchestrator with the provided services
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Gets all agents, converting AgentDefinitions to Agents with default
    /// provider and model, and populates the registry
    pub async fn get_agents(&self) -> Result<Vec<Agent>> {
        // Load definitions from AgentLoader
        let agent_defs = self.services.get_agents().await?;
        let default_provider = self.services.get_default_provider().await?;
        let default_model = self
            .services
            .get_default_model(&default_provider.id)
            .await?;

        let agents: Vec<Agent> = agent_defs
            .into_iter()
            .map(|def| def.into_agent(default_provider.id, default_model.clone()))
            .collect();

        // Populate the runtime registry so other parts of the app can query
        // agents by id efficiently.
        self.services.set_agents(agents.clone()).await?;

        Ok(agents)
    }

    /// Gets a specific agent by ID, converting AgentDefinition to Agent with
    /// default provider and model
    pub async fn get_agent(&self, agent_id: &AgentId) -> Result<Option<Agent>> {
        // First try to get from registry (fast path)
        if let Some(agent) = AgentRegistry::get_agent(&*self.services, agent_id).await? {
            return Ok(Some(agent));
        }

        // Fallback: load from loader and convert
        let agent_def: Option<AgentDefinition> =
            AgentLoader::get_agent(&*self.services, agent_id).await?;

        if let Some(def) = agent_def {
            let default_provider = self.services.get_default_provider().await?;
            let default_model = self
                .services
                .get_default_model(&default_provider.id)
                .await?;
            Ok(Some(def.into_agent(default_provider.id, default_model)))
        } else {
            Ok(None)
        }
    }

    /// Gets the active agent, converting AgentDefinition to Agent with default
    /// provider and model
    pub async fn get_active_agent(&self) -> Result<Option<Agent>> {
        // First try to get from registry (fast path)
        if let Some(agent) = AgentRegistry::get_active_agent(&*self.services).await? {
            return Ok(Some(agent));
        }

        // Fallback: load from loader and convert
        let agent_def: Option<AgentDefinition> =
            AgentLoader::get_active_agent(&*self.services).await?;

        if let Some(def) = agent_def {
            let default_provider = self.services.get_default_provider().await?;
            let default_model = self
                .services
                .get_default_model(&default_provider.id)
                .await?;
            Ok(Some(def.into_agent(default_provider.id, default_model)))
        } else {
            Ok(None)
        }
    }

    /// Sets the active agent ID
    pub async fn set_active_agent_id(&self, agent_id: AgentId) -> Result<()> {
        self.services.set_active_agent_id(agent_id).await
    }

    /// Gets the active agent ID
    pub async fn get_active_agent_id(&self) -> Result<Option<AgentId>> {
        self.services.get_active_agent_id().await
    }
}
