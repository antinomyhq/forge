use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A newtype for snapshot IDs, internally using UUID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SnapshotId(Uuid);

impl SnapshotId {
    /// Create a new random SnapshotId
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a SnapshotId from a string
    pub fn parse(s: &str) -> Option<Self> {
        Uuid::parse_str(s).ok().map(Self)
    }

    /// Get the underlying UUID
    pub fn uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for SnapshotId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for SnapshotId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for SnapshotId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

/// Represents information about a file snapshot
///
/// Contains details about when the snapshot was created,
/// the original file path, the snapshot location, and file size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique ID for the file
    pub id: SnapshotId,
    /// Unix timestamp when the snapshot was created
    pub timestamp: u128,
    /// Original file path that was snapshotted
    pub original_path: String,
    /// Path to the snapshot file
    pub snapshot_path: String,
    /// Content of the file encoded as base64
    pub content: String,
}
