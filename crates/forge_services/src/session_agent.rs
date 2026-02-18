use std::sync::Arc;

use forge_app::{AgentRepository, SessionAgentService};
use forge_domain::{
    Agent, AgentId, AppConfigRepository, ProviderId, ProviderRepository, SessionId,
    SessionRepository,
};

/// Session agent management service
///
/// Handles agent switching and validation within sessions.
/// Orchestrates infrastructure calls to convert AgentDefinition to runtime Agent with model overrides applied.
pub struct ForgeSessionAgentService<I> {
    infra: Arc<I>,
}

impl<I> ForgeSessionAgentService<I> {
    /// Creates a new session agent service
    ///
    /// # Arguments
    /// * `infra` - Infrastructure implementing required repository traits
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<I: SessionRepository + AgentRepository + AppConfigRepository + ProviderRepository>
    SessionAgentService for ForgeSessionAgentService<I>
{
    async fn switch_agent(
        &self,
        session_id: &SessionId,
        agent_id: &AgentId,
    ) -> anyhow::Result<()> {
        // Validate agent exists
        self.validate_agent_switch(agent_id).await?;

        // Load session state
        let state = self
            .infra
            .load_session(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Update agent using setter
        let updated_state = state.agent_id(agent_id.clone());

        // Save session state
        self.infra.save_session(session_id, &updated_state).await?;

        Ok(())
    }

    async fn get_session_agent(&self, session_id: &SessionId) -> anyhow::Result<Agent> {
        // Load session state
        let state = self
            .infra
            .load_session(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Get agent definition from infrastructure
        let agent_defs = self.infra.get_agents().await?;
        let agent_def = agent_defs
            .into_iter()
            .find(|def| def.id == state.agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", state.agent_id))?;

        // Get default provider and model for conversion
        let (default_provider_id, default_model_id) = self.get_defaults().await?;

        // Convert AgentDefinition to runtime Agent
        let mut agent = Agent::from_agent_def(agent_def, default_provider_id, default_model_id);

        // Apply model override from session if set
        if let Some(model_override) = state.model_override {
            agent.model = model_override;
        }

        Ok(agent)
    }

    async fn validate_agent_switch(&self, agent_id: &AgentId) -> anyhow::Result<()> {
        let agent_defs = self.infra.get_agents().await?;
        let exists = agent_defs.iter().any(|def| def.id == *agent_id);

        if !exists {
            anyhow::bail!("Agent not found: {}", agent_id);
        }

        Ok(())
    }

    async fn get_available_agents(&self) -> anyhow::Result<Vec<Agent>> {
        // Get agent definitions
        let agent_defs = self.infra.get_agents().await?;

        // Get default provider and model for conversion
        let (default_provider_id, default_model_id) = self.get_defaults().await?;

        // Convert all definitions to runtime agents
        let agents = agent_defs
            .into_iter()
            .map(|def| Agent::from_agent_def(def, default_provider_id.clone(), default_model_id.clone()))
            .collect();

        Ok(agents)
    }
}

impl<I: SessionRepository + AgentRepository + AppConfigRepository + ProviderRepository>
    ForgeSessionAgentService<I>
{
    /// Helper to get default provider and model from app config
    ///
    /// # Errors
    /// Returns an error if defaults are not configured
    async fn get_defaults(&self) -> anyhow::Result<(ProviderId, forge_domain::ModelId)> {
        let app_config = self.infra.get_app_config().await?;

        let provider_id = app_config
            .provider
            .ok_or_else(|| anyhow::anyhow!("No default provider configured"))?;

        let model_id = app_config
            .model
            .get(&provider_id)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("No default model configured for provider {}", provider_id)
            })?;

        Ok((provider_id, model_id))
    }
}
