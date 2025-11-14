use std::sync::Arc;

use anyhow::Result;
use forge_domain::AgentId;

use crate::services::{AgentLoader, AgentRegistry, AppConfigService};
use crate::{Agent, Services};

/// Orchestrates agent operations including loading definitions, converting to
/// runtime agents, and managing the agent registry.
pub struct AgentOrchestrator<S> {
    services: Arc<S>,
}

impl<S: Services> AgentOrchestrator<S> {
    /// Creates a new AgentOrchestrator with the provided services
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Gets all agents, converting AgentDefinitions to Agents with default
    /// provider and model. Uses cached agents from registry if available,
    /// otherwise loads definitions and populates the registry.
    pub async fn get_agents(&self) -> Result<Vec<Agent>> {
        // try to get from registry
        let cached_agents = AgentRegistry::get_agents(&*self.services).await?;
        if !cached_agents.is_empty() {
            return Ok(cached_agents);
        }

        // load definitions from AgentLoader, convert, and cache
        let agent_defs = AgentLoader::get_agents(&*self.services).await?;
        let default_provider = self.services.get_default_provider().await?;
        let default_model = self
            .services
            .get_default_model(&default_provider.id)
            .await?;

        let agents: Vec<Agent> = agent_defs
            .into_iter()
            .map(|def| Agent::from_agent_def(def, default_provider.id, default_model.clone()))
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

        // Fallback: load all agents and find the one we need
        // This will also populate the cache for future requests
        let agents = self.get_agents().await?;
        Ok(agents.into_iter().find(|a| &a.id == agent_id))
    }
}
