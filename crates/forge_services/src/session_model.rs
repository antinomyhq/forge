/// Service for managing model overrides per session
///
/// This service manages session-scoped model overrides. It does NOT fetch
/// models from providers - that's the responsibility of the app layer which
/// orchestrates multiple services.
///
/// Architecture:
/// - Service: Manages session → model_override mapping (this service)
/// - App layer: Orchestrates SessionModelService + ProviderService to fetch models
use anyhow::Result;
use forge_app::SessionModelService;
use forge_domain::{ModelId, SessionId, SessionRepository};
use std::sync::Arc;

/// Manages model overrides for sessions
///
/// Only manages the mapping of session → model override. Does not depend on
/// other services. The app layer orchestrates combining overrides with provider
/// services to fetch actual model details.
pub struct ForgeSessionModelService<R> {
    repository: Arc<R>,
}

impl<R> ForgeSessionModelService<R> {
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl<R: SessionRepository> SessionModelService for ForgeSessionModelService<R> {
    async fn set_session_model(&self, session_id: &SessionId, model_id: &ModelId) -> Result<()> {
        // Load session state
        let mut state = self
            .repository
            .load_session(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Update model override
        state.model_override = Some(model_id.clone());

        // Save updated state
        self.repository.save_session(session_id, &state).await?;

        Ok(())
    }

    async fn get_effective_model(&self, session_id: &SessionId) -> Result<ModelId> {
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

    async fn clear_model_override(&self, session_id: &SessionId) -> Result<()> {
        // Load session state
        let mut state = self
            .repository
            .load_session(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Clear override
        state.model_override = None;

        // Save updated state
        self.repository.save_session(session_id, &state).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::{AgentId, ConversationId, SessionState};
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // Mock repository for testing
    struct MockRepository {
        sessions: Mutex<HashMap<SessionId, SessionState>>,
    }

    impl MockRepository {
        fn new() -> Self {
            Self { sessions: Mutex::new(HashMap::new()) }
        }

        fn with_session(session_id: SessionId, state: SessionState) -> Self {
            let mut sessions = HashMap::new();
            sessions.insert(session_id, state);
            Self { sessions: Mutex::new(sessions) }
        }
    }

    #[async_trait::async_trait]
    impl SessionRepository for MockRepository {
        async fn save_session(&self, session_id: &SessionId, state: &SessionState) -> Result<()> {
            self.sessions
                .lock()
                .unwrap()
                .insert(session_id.clone(), state.clone());
            Ok(())
        }

        async fn load_session(&self, session_id: &SessionId) -> Result<Option<SessionState>> {
            Ok(self.sessions.lock().unwrap().get(session_id).cloned())
        }

        async fn delete_session(&self, session_id: &SessionId) -> Result<()> {
            self.sessions.lock().unwrap().remove(session_id);
            Ok(())
        }

        async fn list_sessions(&self) -> Result<Vec<SessionId>> {
            Ok(self.sessions.lock().unwrap().keys().cloned().collect())
        }
    }

    fn create_test_session_state() -> SessionState {
        SessionState {
            conversation_id: ConversationId::generate(),
            agent_id: AgentId::new("test-agent"),
            model_override: None,
            created_at: chrono::Utc::now(),
            last_active: chrono::Utc::now(),
            status: forge_domain::SessionStatus::Active,
        }
    }

    #[tokio::test]
    async fn test_set_session_model() {
        let session_id = SessionId::from_u64(1);
        let state = create_test_session_state();
        let repo = Arc::new(MockRepository::with_session(session_id, state));
        let service = ForgeSessionModelService::new(repo.clone());

        let model_id = ModelId::new("custom-model".to_string());

        let actual = service.set_session_model(&session_id, &model_id).await;

        assert!(actual.is_ok());

        // Verify the override was saved
        let saved_state = repo.load_session(&session_id).await.unwrap().unwrap();
        assert_eq!(saved_state.model_override, Some(model_id));
    }

    #[tokio::test]
    async fn test_get_effective_model_with_override() {
        let session_id = SessionId::from_u64(1);
        let mut state = create_test_session_state();
        let override_model = ModelId::new("override-model".to_string());
        state.model_override = Some(override_model.clone());

        let repo = Arc::new(MockRepository::with_session(session_id, state));
        let service = ForgeSessionModelService::new(repo);

        let actual = service.get_effective_model(&session_id).await.unwrap();

        assert_eq!(actual, override_model);
    }

    #[tokio::test]
    async fn test_get_effective_model_without_override() {
        let session_id = SessionId::from_u64(1);
        let state = create_test_session_state();

        let repo = Arc::new(MockRepository::with_session(session_id, state));
        let service = ForgeSessionModelService::new(repo);

        let actual = service.get_effective_model(&session_id).await;

        // Should return error when no override is set
        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("No model override set")
        );
    }

    #[tokio::test]
    async fn test_clear_model_override() {
        let session_id = SessionId::from_u64(1);
        let mut state = create_test_session_state();
        state.model_override = Some(ModelId::new("override-model".to_string()));

        let repo = Arc::new(MockRepository::with_session(session_id, state));
        let service = ForgeSessionModelService::new(repo.clone());

        let actual = service.clear_model_override(&session_id).await;

        assert!(actual.is_ok());

        // Verify the override was cleared
        let saved_state = repo.load_session(&session_id).await.unwrap().unwrap();
        assert_eq!(saved_state.model_override, None);
    }

    #[tokio::test]
    async fn test_set_model_on_nonexistent_session() {
        let session_id = SessionId::from_u64(999);
        let repo = Arc::new(MockRepository::new());
        let service = ForgeSessionModelService::new(repo);

        let model_id = ModelId::new("custom-model".to_string());
        let actual = service.set_session_model(&session_id, &model_id).await;

        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("Session not found")
        );
    }
}
