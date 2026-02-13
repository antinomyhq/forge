use std::sync::Arc;

use forge_app::SessionService;
use forge_domain::{AgentId, SessionContext, SessionId, SessionState};

/// Session management service
///
/// Manages session lifecycle, state persistence, and cancellation.
pub struct ForgeSessionService<F> {
    _infra: Arc<F>,
}

impl<F> ForgeSessionService<F> {
    /// Creates a new session service
    pub fn new(infra: Arc<F>) -> Self {
        Self { _infra: infra }
    }
}

#[async_trait::async_trait]
impl<F: Send + Sync> SessionService for ForgeSessionService<F> {
    async fn create_session(&self, _agent_id: AgentId) -> anyhow::Result<SessionId> {
        anyhow::bail!("SessionService not yet implemented")
    }

    async fn get_session_state(&self, _session_id: &SessionId) -> anyhow::Result<SessionState> {
        anyhow::bail!("SessionService not yet implemented")
    }

    async fn get_session_context(&self, _session_id: &SessionId) -> anyhow::Result<SessionContext> {
        anyhow::bail!("SessionService not yet implemented")
    }

    async fn update_session_state(
        &self,
        _session_id: &SessionId,
        _state: SessionState,
    ) -> anyhow::Result<()> {
        anyhow::bail!("SessionService not yet implemented")
    }

    async fn delete_session(&self, _session_id: &SessionId) -> anyhow::Result<()> {
        anyhow::bail!("SessionService not yet implemented")
    }

    async fn list_sessions(&self) -> anyhow::Result<Vec<(SessionId, SessionState)>> {
        anyhow::bail!("SessionService not yet implemented")
    }

    async fn cancel_session(&self, _session_id: &SessionId) -> anyhow::Result<()> {
        anyhow::bail!("SessionService not yet implemented")
    }
}
