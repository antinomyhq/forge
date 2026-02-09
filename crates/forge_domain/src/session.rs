use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::{AgentId, ConversationId, ModelId};

/// Unique identifier for a session
///
/// A session represents a client connection that can span multiple prompts
/// and maintain state like active agent and model overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(uuid::Uuid);

impl SessionId {
    /// Creates a new random SessionId
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Creates a SessionId from a u64 (for testing or migration)
    pub fn from_u64(id: u64) -> Self {
        Self(uuid::Uuid::from_u128(id as u128))
    }

    /// Creates a SessionId from a string representation
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID
    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(uuid::Uuid::parse_str(s)?))
    }

    /// Returns the inner UUID
    pub fn as_uuid(&self) -> uuid::Uuid {
        self.0
    }

    /// Returns the string representation
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<uuid::Uuid> for SessionId {
    fn from(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }
}

/// State associated with a session
///
/// Tracks the conversation, active agent, and any model overrides for the
/// session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// The conversation ID associated with this session
    pub conversation_id: ConversationId,

    /// The active agent for this session
    pub agent_id: AgentId,

    /// Optional model override for this session
    ///
    /// When set, this model is used instead of the agent's default model.
    pub model_override: Option<ModelId>,

    /// When the session was created
    pub created_at: DateTime<Utc>,

    /// When the session was last active
    pub last_active: DateTime<Utc>,

    /// Session status
    pub status: SessionStatus,
}

impl SessionState {
    /// Creates a new SessionState
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

    /// Checks if the session is expired based on TTL
    pub fn is_expired(&self, ttl_seconds: i64) -> bool {
        let elapsed = Utc::now().signed_duration_since(self.last_active);
        elapsed.num_seconds() > ttl_seconds
    }
}

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    /// Session is active and can accept prompts
    Active,
    /// Session has been explicitly closed
    Closed,
    /// Session has expired due to inactivity
    Expired,
}

/// Context for a session including runtime state
///
/// This is a non-persistent wrapper around SessionState that includes
/// runtime-only state like cancellation tokens.
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

impl SessionContext {
    /// Creates a new SessionContext
    pub fn new(state: SessionState) -> Self {
        Self {
            state,
            cancellation_token: CancellationToken::new(),
        }
    }

    /// Creates a SessionContext from components
    pub fn with_token(state: SessionState, token: CancellationToken) -> Self {
        Self {
            state,
            cancellation_token: token,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_session_id_generation() {
        let fixture_id1 = SessionId::generate();
        let fixture_id2 = SessionId::generate();

        assert_ne!(fixture_id1, fixture_id2, "Generated IDs should be unique");
    }

    #[test]
    fn test_session_id_from_u64() {
        let fixture_id = 42u64;

        let actual = SessionId::from_u64(fixture_id);
        let expected = SessionId::from_u64(42);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_session_id_from_string() {
        let fixture_uuid = uuid::Uuid::new_v4();
        let fixture_str = fixture_uuid.to_string();

        let actual = SessionId::from_string(&fixture_str).unwrap();
        let expected = SessionId::from(fixture_uuid);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_session_id_display() {
        let fixture_uuid = uuid::Uuid::new_v4();
        let fixture_id = SessionId::from(fixture_uuid);

        let actual = format!("{}", fixture_id);
        let expected = fixture_uuid.to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_session_state_new() {
        let fixture_conv_id = ConversationId::generate();
        let fixture_agent_id = AgentId::default();

        let actual = SessionState::new(fixture_conv_id, fixture_agent_id.clone());

        assert_eq!(actual.conversation_id, fixture_conv_id);
        assert_eq!(actual.agent_id, fixture_agent_id);
        assert_eq!(actual.model_override, None);
        assert_eq!(actual.status, SessionStatus::Active);
    }

    #[test]
    fn test_session_state_touch() {
        let fixture_conv_id = ConversationId::generate();
        let fixture_agent_id = AgentId::default();
        let mut fixture_state = SessionState::new(fixture_conv_id, fixture_agent_id);

        let before = fixture_state.last_active;
        std::thread::sleep(std::time::Duration::from_millis(10));
        fixture_state.touch();
        let after = fixture_state.last_active;

        assert!(after > before, "Last active should be updated");
    }

    #[test]
    fn test_session_state_is_expired() {
        let fixture_conv_id = ConversationId::generate();
        let fixture_agent_id = AgentId::default();
        let mut fixture_state = SessionState::new(fixture_conv_id, fixture_agent_id);

        // Set last_active to 2 hours ago
        fixture_state.last_active = Utc::now() - chrono::Duration::hours(2);

        let actual_expired = fixture_state.is_expired(3600); // 1 hour TTL
        let actual_not_expired = fixture_state.is_expired(10800); // 3 hour TTL

        assert!(actual_expired, "Should be expired with 1 hour TTL");
        assert!(!actual_not_expired, "Should not be expired with 3 hour TTL");
    }

    #[test]
    fn test_session_context_new() {
        let fixture_conv_id = ConversationId::generate();
        let fixture_agent_id = AgentId::default();
        let fixture_state = SessionState::new(fixture_conv_id, fixture_agent_id.clone());

        let actual = SessionContext::new(fixture_state.clone());

        assert_eq!(actual.state.conversation_id, fixture_conv_id);
        assert_eq!(actual.state.agent_id, fixture_agent_id);
        assert!(!actual.cancellation_token.is_cancelled());
    }

    #[test]
    fn test_session_context_cancellation() {
        let fixture_conv_id = ConversationId::generate();
        let fixture_agent_id = AgentId::default();
        let fixture_state = SessionState::new(fixture_conv_id, fixture_agent_id);
        let fixture_context = SessionContext::new(fixture_state);

        fixture_context.cancellation_token.cancel();

        assert!(fixture_context.cancellation_token.is_cancelled());
    }
}
