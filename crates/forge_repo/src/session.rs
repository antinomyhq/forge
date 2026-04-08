use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use forge_domain::{SessionId, SessionRepository, SessionState};

/// In-memory session repository implementation
///
/// This repository stores session state in memory using a HashMap.
/// Sessions are not persisted to disk and will be lost on application restart.
pub struct ForgeSessionRepository {
    sessions: Mutex<HashMap<SessionId, SessionState>>,
}

impl ForgeSessionRepository {
    /// Creates a new in-memory session repository
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for ForgeSessionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SessionRepository for ForgeSessionRepository {
    async fn save_session(
        &self,
        session_id: &SessionId,
        state: &SessionState,
    ) -> anyhow::Result<()> {
        self.sessions
            .lock()
            .unwrap()
            .insert(*session_id, state.clone());
        Ok(())
    }

    async fn load_session(&self, session_id: &SessionId) -> anyhow::Result<Option<SessionState>> {
        Ok(self.sessions.lock().unwrap().get(session_id).cloned())
    }

    async fn delete_session(&self, session_id: &SessionId) -> anyhow::Result<()> {
        self.sessions.lock().unwrap().remove(session_id);
        Ok(())
    }

    async fn list_sessions(&self) -> anyhow::Result<Vec<SessionId>> {
        Ok(self.sessions.lock().unwrap().keys().copied().collect())
    }

    async fn cleanup_expired_sessions(&self, ttl: Duration) -> anyhow::Result<usize> {
        let ttl_seconds = ttl.as_secs() as i64;
        let mut sessions = self.sessions.lock().unwrap();
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
