use std::sync::Arc;

use forge_app::SessionModelService;
use forge_domain::{ModelId, SessionId, SessionRepository};

/// Session model management service
///
/// Manages session-specific model overrides and effective model resolution.
/// This service manages session → model_override mapping only.
/// Does not depend on other services. The app layer orchestrates combining overrides
/// with provider services to fetch actual model details.
pub struct ForgeSessionModelService<R> {
    repository: Arc<R>,
}

impl<R> ForgeSessionModelService<R> {
    /// Creates a new session model service
    ///
    /// # Arguments
    /// * `repository` - Repository for session persistence
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl<R: SessionRepository> SessionModelService for ForgeSessionModelService<R> {
    async fn set_session_model(
        &self,
        session_id: &SessionId,
        model_id: &ModelId,
    ) -> anyhow::Result<()> {
        // Load session state
        let state = self
            .repository
            .load_session(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Update model override using setter
        let updated_state = state.model_override(model_id.clone());

        // Save updated state
        self.repository.save_session(session_id, &updated_state).await?;

        Ok(())
    }

    async fn get_effective_model(&self, session_id: &SessionId) -> anyhow::Result<ModelId> {
        // Load session state
        let state = self
            .repository
            .load_session(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Return override if set
        // If no override, the app layer should get the agent's model
        state
            .model_override
            .ok_or_else(|| anyhow::anyhow!("No model override set for session. App layer should get agent's default model via SessionAgentService"))
    }

    async fn clear_model_override(&self, session_id: &SessionId) -> anyhow::Result<()> {
        // Load session state
        let mut state = self
            .repository
            .load_session(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Clear override directly (setters don't work well for clearing Options)
        state.model_override = None;

        // Save updated state
        self.repository.save_session(session_id, &state).await?;

        Ok(())
    }
}
