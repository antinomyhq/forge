//! In-memory session repository implementation

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use forge_domain::{SessionId, SessionRepository, SessionState};
use tokio::sync::Mutex;

/// In-memory implementation of SessionRepository
///
/// This implementation stores sessions in memory and does not persist them
/// across application restarts. Suitable for temporary session management.
#[derive(Clone)]
pub struct InMemorySessionRepository {
    sessions: Arc<Mutex<HashMap<SessionId, SessionState>>>,
}

impl InMemorySessionRepository {
    /// Creates a new in-memory session repository
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySessionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SessionRepository for InMemorySessionRepository {
    async fn save_session(&self, session_id: &SessionId, state: &SessionState) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        sessions.insert(*session_id, state.clone());
        Ok(())
    }

    async fn load_session(&self, session_id: &SessionId) -> Result<Option<SessionState>> {
        let sessions = self.sessions.lock().await;
        Ok(sessions.get(session_id).cloned())
    }

    async fn delete_session(&self, session_id: &SessionId) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        sessions.remove(session_id);
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionId>> {
        let sessions = self.sessions.lock().await;
        Ok(sessions.keys().copied().collect())
    }

    async fn cleanup_expired_sessions(&self, ttl: Duration) -> Result<usize> {
        let mut sessions = self.sessions.lock().await;
        let ttl_secs = ttl.as_secs() as i64;

        let expired: Vec<SessionId> = sessions
            .iter()
            .filter(|(_, state)| state.is_expired(ttl_secs))
            .map(|(id, _)| *id)
            .collect();

        let count = expired.len();
        for id in expired {
            sessions.remove(&id);
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::{AgentId, ConversationId};
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn test_save_and_load_session() {
        let fixture = InMemorySessionRepository::new();
        let session_id = SessionId::from_u64(1);
        let state = SessionState::new(ConversationId::generate(), AgentId::new("test"));

        fixture.save_session(&session_id, &state).await.unwrap();

        let actual = fixture.load_session(&session_id).await.unwrap();
        let expected = Some(state);

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let fixture = InMemorySessionRepository::new();
        let session_id = SessionId::from_u64(1);
        let state = SessionState::new(ConversationId::generate(), AgentId::new("test"));

        fixture.save_session(&session_id, &state).await.unwrap();
        fixture.delete_session(&session_id).await.unwrap();

        let actual = fixture.load_session(&session_id).await.unwrap();
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let fixture = InMemorySessionRepository::new();
        let session_id1 = SessionId::from_u64(1);
        let session_id2 = SessionId::from_u64(2);
        let state = SessionState::new(ConversationId::generate(), AgentId::new("test"));

        fixture.save_session(&session_id1, &state).await.unwrap();
        fixture.save_session(&session_id2, &state).await.unwrap();

        let actual = fixture.list_sessions().await.unwrap();

        assert_eq!(actual.len(), 2);
        assert!(actual.contains(&session_id1));
        assert!(actual.contains(&session_id2));
    }
}
