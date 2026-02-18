use std::sync::Arc;

use forge_app::SessionService;
use forge_domain::{AgentId, ConversationId, SessionContext, SessionId, SessionRepository, SessionState};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Session management service
///
/// Manages session lifecycle, state persistence, and cancellation.
/// Sessions are stored in memory for fast access and optionally persisted to storage via SessionRepository.
pub struct ForgeSessionService<R> {
    repository: Arc<R>,
    /// In-memory session cache for fast access
    sessions: Arc<Mutex<std::collections::HashMap<SessionId, SessionState>>>,
    /// In-memory cancellation tokens (not persisted)
    cancellation_tokens: Arc<Mutex<std::collections::HashMap<SessionId, CancellationToken>>>,
}

impl<R> ForgeSessionService<R> {
    /// Creates a new session service
    ///
    /// # Arguments
    /// * `repository` - Repository for session persistence
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
            cancellation_tokens: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl<R: SessionRepository> SessionService for ForgeSessionService<R> {
    async fn create_session(&self, agent_id: AgentId) -> anyhow::Result<SessionId> {
        let session_id = SessionId::generate();
        let conversation_id = ConversationId::generate();
        let state = SessionState::new(conversation_id, agent_id);

        // Store in memory
        self.sessions.lock().await.insert(session_id, state.clone());

        // Create cancellation token
        self.cancellation_tokens
            .lock()
            .await
            .insert(session_id, CancellationToken::new());

        // Persist to storage
        self.repository.save_session(&session_id, &state).await?;

        Ok(session_id)
    }

    async fn get_session_state(&self, session_id: &SessionId) -> anyhow::Result<SessionState> {
        self.get_session_state_internal(session_id).await
    }

    async fn get_session_context(&self, session_id: &SessionId) -> anyhow::Result<SessionContext> {
        let state = self.get_session_state_internal(session_id).await?;

        let cancellation_token = self
            .cancellation_tokens
            .lock()
            .await
            .get(session_id)
            .cloned()
            .unwrap_or_else(CancellationToken::new);

        Ok(SessionContext {
            state,
            cancellation_token,
        })
    }

    async fn update_session_state(
        &self,
        session_id: &SessionId,
        state: SessionState,
    ) -> anyhow::Result<()> {
        self.update_session_state_internal(session_id, state).await
    }

    async fn delete_session(&self, session_id: &SessionId) -> anyhow::Result<()> {
        // Remove from memory
        self.sessions.lock().await.remove(session_id);
        self.cancellation_tokens.lock().await.remove(session_id);

        // Remove from storage
        self.repository.delete_session(session_id).await?;

        Ok(())
    }

    async fn list_sessions(&self) -> anyhow::Result<Vec<(SessionId, SessionState)>> {
        let session_ids = self.repository.list_sessions().await?;
        let mut result = Vec::new();

        for session_id in session_ids {
            if let Ok(state) = self.get_session_state_internal(&session_id).await {
                result.push((session_id, state));
            }
        }

        Ok(result)
    }

    async fn cancel_session(&self, session_id: &SessionId) -> anyhow::Result<()> {
        let mut tokens = self.cancellation_tokens.lock().await;
        if let Some(token) = tokens.get(session_id) {
            token.cancel();
            // Replace with a new token so future prompts can run
            tokens.insert(*session_id, CancellationToken::new());
        }
        Ok(())
    }
}

impl<R: SessionRepository> ForgeSessionService<R> {
    /// Internal method to get session state (used by both trait and internal methods)
    async fn get_session_state_internal(&self, session_id: &SessionId) -> anyhow::Result<SessionState> {
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

    /// Internal method to update session state
    async fn update_session_state_internal(
        &self,
        session_id: &SessionId,
        state: SessionState,
    ) -> anyhow::Result<()> {
        // Update memory
        self.sessions.lock().await.insert(*session_id, state.clone());

        // Persist to storage
        self.repository.save_session(session_id, &state).await?;

        Ok(())
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
    pub async fn cleanup_expired_sessions(&self, ttl: std::time::Duration) -> anyhow::Result<usize> {
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
    pub async fn touch_session(&self, session_id: &SessionId) -> anyhow::Result<()> {
        let mut state = self.get_session_state_internal(session_id).await?;
        state.touch();
        self.update_session_state_internal(session_id, state).await
    }
}
