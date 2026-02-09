use anyhow::Result;
use forge_domain::{AgentId, ConversationId, ModelId, SessionId, SessionRepository, SessionState};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Service for managing session lifecycle and state
///
/// Provides operations for creating, retrieving, updating, and deleting sessions.
/// Sessions are stored in memory and optionally persisted to storage via SessionRepository.
pub struct ForgeSessionService<R> {
    repository: Arc<R>,
    // In-memory session cache for fast access
    sessions: Arc<Mutex<std::collections::HashMap<SessionId, SessionState>>>,
}

impl<R> ForgeSessionService<R> {
    /// Creates a new ForgeSessionService
    ///
    /// # Arguments
    /// * `repository` - Repository for session persistence
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

impl<R: SessionRepository> ForgeSessionService<R> {
    /// Creates a new session
    ///
    /// # Arguments
    /// * `conversation_id` - The conversation ID for this session
    /// * `agent_id` - The agent ID for this session
    ///
    /// # Errors
    /// Returns an error if session creation or persistence fails
    pub async fn create_session(
        &self,
        conversation_id: ConversationId,
        agent_id: AgentId,
    ) -> Result<SessionId> {
        let session_id = SessionId::generate();
        let state = SessionState::new(conversation_id, agent_id);

        // Store in memory
        self.sessions.lock().await.insert(session_id, state.clone());

        // Persist to storage
        self.repository.save_session(&session_id, &state).await?;

        Ok(session_id)
    }

    /// Retrieves the state of a session
    ///
    /// # Arguments
    /// * `session_id` - The ID of the session to retrieve
    ///
    /// # Errors
    /// Returns an error if the session doesn't exist or retrieval fails
    pub async fn get_session_state(&self, session_id: &SessionId) -> Result<SessionState> {
        // Try memory first
        if let Some(state) = self.sessions.lock().await.get(session_id) {
            return Ok(state.clone());
        }

        // Fall back to repository
        if let Some(state) = self.repository.load_session(session_id).await? {
            // Cache it
            self.sessions.lock().await.insert(*session_id, state.clone());
            return Ok(state);
        }

        anyhow::bail!("Session not found: {}", session_id)
    }

    /// Updates the state of a session
    ///
    /// # Arguments
    /// * `session_id` - The ID of the session to update
    /// * `state` - The new session state
    ///
    /// # Errors
    /// Returns an error if the update or persistence fails
    pub async fn update_session_state(
        &self,
        session_id: &SessionId,
        state: SessionState,
    ) -> Result<()> {
        // Update memory
        self.sessions.lock().await.insert(*session_id, state.clone());

        // Persist to storage
        self.repository.save_session(session_id, &state).await?;

        Ok(())
    }

    /// Deletes a session
    ///
    /// # Arguments
    /// * `session_id` - The ID of the session to delete
    ///
    /// # Errors
    /// Returns an error if deletion fails
    pub async fn delete_session(&self, session_id: &SessionId) -> Result<()> {
        // Remove from memory
        self.sessions.lock().await.remove(session_id);

        // Remove from storage
        self.repository.delete_session(session_id).await?;

        Ok(())
    }

    /// Lists all active sessions
    ///
    /// # Errors
    /// Returns an error if listing fails
    pub async fn list_sessions(&self) -> Result<Vec<SessionId>> {
        self.repository.list_sessions().await
    }

    /// Cleans up expired sessions
    ///
    /// # Arguments
    /// * `ttl` - Time-to-live duration for session expiration
    ///
    /// # Returns
    /// The number of sessions cleaned up
    ///
    /// # Errors
    /// Returns an error if cleanup fails
    pub async fn cleanup_expired_sessions(&self, ttl: Duration) -> Result<usize> {
        let ttl_seconds = ttl.as_secs() as i64;
        let count = self.repository.cleanup_expired_sessions(ttl).await?;

        // Also clean up memory cache
        let mut sessions = self.sessions.lock().await;
        let expired: Vec<SessionId> = sessions
            .iter()
            .filter(|(_, state)| state.is_expired(ttl_seconds))
            .map(|(id, _)| *id)
            .collect();

        for id in &expired {
            sessions.remove(id);
        }

        Ok(count)
    }

    /// Touches a session to update its last activity timestamp
    ///
    /// # Arguments
    /// * `session_id` - The ID of the session to touch
    ///
    /// # Errors
    /// Returns an error if the session doesn't exist or update fails
    pub async fn touch_session(&self, session_id: &SessionId) -> Result<()> {
        let mut state = self.get_session_state(session_id).await?;
        state.touch();
        self.update_session_state(session_id, state).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;

    // Mock repository for testing
    struct MockSessionRepository {
        sessions: Arc<Mutex<HashMap<SessionId, SessionState>>>,
    }

    impl MockSessionRepository {
        fn new() -> Self {
            Self {
                sessions: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl SessionRepository for MockSessionRepository {
        async fn save_session(&self, session_id: &SessionId, state: &SessionState) -> Result<()> {
            self.sessions.lock().await.insert(*session_id, state.clone());
            Ok(())
        }

        async fn load_session(&self, session_id: &SessionId) -> Result<Option<SessionState>> {
            Ok(self.sessions.lock().await.get(session_id).cloned())
        }

        async fn delete_session(&self, session_id: &SessionId) -> Result<()> {
            self.sessions.lock().await.remove(session_id);
            Ok(())
        }

        async fn list_sessions(&self) -> Result<Vec<SessionId>> {
            Ok(self.sessions.lock().await.keys().copied().collect())
        }

        async fn cleanup_expired_sessions(&self, ttl: Duration) -> Result<usize> {
            let ttl_seconds = ttl.as_secs() as i64;
            let mut sessions = self.sessions.lock().await;
            let expired: Vec<SessionId> = sessions
                .iter()
                .filter(|(_, state)| state.is_expired(ttl_seconds))
                .map(|(id, _)| *id)
                .collect();

            for id in &expired {
                sessions.remove(id);
            }

            Ok(expired.len())
        }
    }

    #[tokio::test]
    async fn test_create_session() {
        let fixture = ForgeSessionService::new(Arc::new(MockSessionRepository::new()));
        let conversation_id = ConversationId::generate();
        let agent_id = AgentId::new("test-agent");

        let actual = fixture
            .create_session(conversation_id, agent_id.clone())
            .await
            .unwrap();

        // Verify session was created
        let state = fixture.get_session_state(&actual).await.unwrap();
        assert_eq!(state.conversation_id, conversation_id);
        assert_eq!(state.agent_id, agent_id);
    }

    #[tokio::test]
    async fn test_get_session_state() {
        let fixture = ForgeSessionService::new(Arc::new(MockSessionRepository::new()));
        let conversation_id = ConversationId::generate();
        let agent_id = AgentId::new("test-agent");
        let session_id = fixture
            .create_session(conversation_id, agent_id)
            .await
            .unwrap();

        let actual = fixture.get_session_state(&session_id).await.unwrap();

        assert_eq!(actual.conversation_id, conversation_id);
    }

    #[tokio::test]
    async fn test_update_session_state() {
        let fixture = ForgeSessionService::new(Arc::new(MockSessionRepository::new()));
        let conversation_id = ConversationId::generate();
        let agent_id = AgentId::new("test-agent");
        let session_id = fixture
            .create_session(conversation_id, agent_id)
            .await
            .unwrap();

        let mut state = fixture.get_session_state(&session_id).await.unwrap();
        state.model_override = Some(ModelId::new("new-model".to_string()));
        fixture
            .update_session_state(&session_id, state.clone())
            .await
            .unwrap();

        let actual = fixture.get_session_state(&session_id).await.unwrap();
        assert_eq!(
            actual.model_override,
            Some(ModelId::new("new-model".to_string()))
        );
    }

    #[tokio::test]
    async fn test_delete_session() {
        let fixture = ForgeSessionService::new(Arc::new(MockSessionRepository::new()));
        let conversation_id = ConversationId::generate();
        let agent_id = AgentId::new("test-agent");
        let session_id = fixture
            .create_session(conversation_id, agent_id)
            .await
            .unwrap();

        fixture.delete_session(&session_id).await.unwrap();

        let result = fixture.get_session_state(&session_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let fixture = ForgeSessionService::new(Arc::new(MockSessionRepository::new()));
        let conversation_id1 = ConversationId::generate();
        let conversation_id2 = ConversationId::generate();
        let agent_id = AgentId::new("test-agent");

        let id1 = fixture
            .create_session(conversation_id1, agent_id.clone())
            .await
            .unwrap();
        let id2 = fixture
            .create_session(conversation_id2, agent_id)
            .await
            .unwrap();

        let actual = fixture.list_sessions().await.unwrap();

        assert_eq!(actual.len(), 2);
        assert!(actual.contains(&id1));
        assert!(actual.contains(&id2));
    }

    #[tokio::test]
    async fn test_cleanup_expired_sessions() {
        let fixture = ForgeSessionService::new(Arc::new(MockSessionRepository::new()));
        let conversation_id = ConversationId::generate();
        let agent_id = AgentId::new("test-agent");
        fixture
            .create_session(conversation_id, agent_id)
            .await
            .unwrap();

        // Wait a bit to ensure session is old enough
        tokio::time::sleep(Duration::from_millis(10)).await;

        let actual = fixture
            .cleanup_expired_sessions(Duration::from_millis(5))
            .await
            .unwrap();

        assert_eq!(actual, 1);
    }

    #[tokio::test]
    async fn test_touch_session() {
        let fixture = ForgeSessionService::new(Arc::new(MockSessionRepository::new()));
        let conversation_id = ConversationId::generate();
        let agent_id = AgentId::new("test-agent");
        let session_id = fixture
            .create_session(conversation_id, agent_id)
            .await
            .unwrap();

        let state_before = fixture.get_session_state(&session_id).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;

        fixture.touch_session(&session_id).await.unwrap();

        let state_after = fixture.get_session_state(&session_id).await.unwrap();
        assert!(state_after.last_active > state_before.last_active);
    }
}
