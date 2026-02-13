use std::sync::Arc;

use forge_app::SessionModelService;
use forge_domain::{ModelId, SessionId};

/// Session model management service
///
/// Manages session-specific model overrides and effective model resolution.
pub struct ForgeSessionModelService<F> {
    _infra: Arc<F>,
}

impl<F> ForgeSessionModelService<F> {
    /// Creates a new session model service
    pub fn new(infra: Arc<F>) -> Self {
        Self { _infra: infra }
    }
}

#[async_trait::async_trait]
impl<F: Send + Sync> SessionModelService for ForgeSessionModelService<F> {
    async fn set_session_model(
        &self,
        _session_id: &SessionId,
        _model_id: &ModelId,
    ) -> anyhow::Result<()> {
        anyhow::bail!("SessionModelService not yet implemented")
    }

    async fn get_effective_model(&self, _session_id: &SessionId) -> anyhow::Result<ModelId> {
        anyhow::bail!("SessionModelService not yet implemented")
    }

    async fn clear_model_override(&self, _session_id: &SessionId) -> anyhow::Result<()> {
        anyhow::bail!("SessionModelService not yet implemented")
    }
}
