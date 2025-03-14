use serde::{Deserialize, Serialize};

/// Represents information about a file snapshot
///
/// Contains details about when the snapshot was created,
/// the original file path, the snapshot location, and file size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique hash ID for the snapshot (based on file path and timestamp)
    pub hash: String,
    /// Unix timestamp when the snapshot was created
    pub timestamp: u128,
    /// Original file path that was snapshotted
    pub original_path: String,
    /// Path to the snapshot file
    pub snapshot_path: String,
    /// Content of the file encoded as base64
    pub content: String,
}
