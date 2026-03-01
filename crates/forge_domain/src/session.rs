use chrono::{DateTime, Utc};
use derive_more::{Display, From};
use tokio_util::sync::CancellationToken;

use crate::{AgentId, ConversationId, ModelId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, From)]
pub struct SessionId(uuid::Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// Session is active and can accept prompts
    Active,
    /// Session has been explicitly closed
    Closed,
    /// Session has expired due to inactivity
    Expired,
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub conversation_id: ConversationId,
    pub agent_id: AgentId,
    pub model_override: Option<ModelId>,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone)]
pub struct SessionContext {
    /// The session state
    pub state: SessionState,

    /// Cancellation token for this session
    ///
    /// Used to cancel long-running operations when the session is closed
    /// or the client disconnects.
    pub cancellation_token: CancellationToken,
}

impl SessionId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn from_u64(id: u64) -> Self {
        Self(uuid::Uuid::from_u128(id as u128))
    }

    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(uuid::Uuid::parse_str(s)?))
    }

    pub fn as_uuid(&self) -> uuid::Uuid {
        self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl SessionState {
    pub fn new(conversation_id: ConversationId, agent_id: AgentId) -> Self {
        let now = Utc::now();
        Self {
            conversation_id,
            agent_id,
            model_override: None,
            created_at: now,
            last_active: now,
            status: SessionStatus::Active,
        }
    }

    /// Updates the last active timestamp
    pub fn touch(&mut self) {
        self.last_active = Utc::now();
    }

    pub fn is_expired(&self, ttl_seconds: i64) -> bool {
        let elapsed = Utc::now().signed_duration_since(self.last_active);
        elapsed.num_seconds() > ttl_seconds
    }
}
