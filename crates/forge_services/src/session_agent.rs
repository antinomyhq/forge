use std::sync::Arc;

use forge_app::SessionAgentService;
use forge_domain::{Agent, AgentId, SessionId};

/// Session agent management service
///
/// Handles agent switching and validation within sessions.
pub struct ForgeSessionAgentService<F> {
    _infra: Arc<F>,
}

impl<F> ForgeSessionAgentService<F> {
    /// Creates a new session agent service
    pub fn new(infra: Arc<F>) -> Self {
        Self { _infra: infra }
    }
}

#[async_trait::async_trait]
impl<F: Send + Sync> SessionAgentService for ForgeSessionAgentService<F> {
    async fn switch_agent(
        &self,
        _session_id: &SessionId,
        _agent_id: &AgentId,
    ) -> anyhow::Result<()> {
        anyhow::bail!("SessionAgentService not yet implemented")
    }

    async fn get_session_agent(&self, _session_id: &SessionId) -> anyhow::Result<Agent> {
        anyhow::bail!("SessionAgentService not yet implemented")
    }

    async fn validate_agent_switch(&self, _agent_id: &AgentId) -> anyhow::Result<()> {
        anyhow::bail!("SessionAgentService not yet implemented")
    }

    async fn get_available_agents(&self) -> anyhow::Result<Vec<Agent>> {
        anyhow::bail!("SessionAgentService not yet implemented")
    }
}
