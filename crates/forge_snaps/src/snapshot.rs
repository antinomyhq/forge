use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::hash::Hasher;
use anyhow::{Result, Context};

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

impl Snapshot {
    /// Decode the base64-encoded content of the snapshot
    pub fn decode_content(&self) -> Result<Vec<u8>, base64::DecodeError> {
        use base64::engine::general_purpose;
        use base64::Engine;
        general_purpose::STANDARD.decode(&self.content)
    }

    /// Create a hash of a file path for storage
    pub fn path_hash(path_str: &str) -> String {
        let mut hasher = fnv_rs::Fnv64::default();
        hasher.write(path_str.as_bytes());
        format!("{:x}", hasher.finish())
    }

    /// Create a snapshot filename from a path and timestamp
    pub fn create_snapshot_filename(base_dir: &Path, path: &str, timestamp: u128) -> String {
        base_dir
            .join(path)
            .join(format!("{}.json", timestamp))
            .display()
            .to_string()
    }
    
    /// Check if this snapshot is older than the specified number of days
    pub fn is_older_than_days(&self, days: u32) -> Result<bool> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("Failed to get timestamp")?
            .as_millis();
        
        let threshold = now - (days as u128 * 24 * 60 * 60 * 1000);
        Ok(self.timestamp < threshold)
    }
}