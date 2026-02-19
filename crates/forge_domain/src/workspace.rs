use derive_more::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Workspace identifier (UUID) from workspace server.
///
/// Generated locally and sent to server during CreateWorkspace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[display("{}", _0)]
pub struct WorkspaceId(Uuid);

impl WorkspaceId {
    /// Generate a new random workspace ID
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a workspace ID from a string
    ///
    /// # Errors
    /// Returns an error if the string is not a valid UUID
    pub fn from_string(s: &str) -> anyhow::Result<Self> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the inner UUID
    pub fn inner(&self) -> Uuid {
        self.0
    }
}
