use std::path::PathBuf;

/// Represents information about a file snapshot
///
/// Contains details about when the snapshot was created,
/// the original file path, the snapshot location, and file size.
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// Unix timestamp when the snapshot was created
    pub timestamp: String,
    /// Original file path that was snapshotted
    pub original_path: PathBuf,
    /// Path to the snapshot file
    pub snapshot_path: PathBuf,
    /// Index of this snapshot in the list (0 = newest)
    pub index: usize,
}

impl SnapshotInfo {
    /// Creates a SnapshotInfo with a specific timestamp
    pub fn with_timestamp(
        timestamp: String,
        original_path: PathBuf,
        snapshot_path: PathBuf,
        index: usize,
    ) -> Self {
        Self { timestamp, original_path, snapshot_path, index }
    }

    /// Returns a formatted date string for the snapshot's timestamp
    pub fn formatted_date(&self) -> String {
        // In a real implementation, this would convert the Unix timestamp
        // to a human-readable date string
        self.timestamp.to_string()
    }

    /*    /// Returns a human-readable size string (e.g., "2.4K")
    pub fn formatted_size(&self) -> String {
        if self.size < 1024 {
            format!("{}B", self.size)
        } else if self.size < 1024 * 1024 {
            format!("{:.1}K", self.size as f64 / 1024.0)
        } else if self.size < 1024 * 1024 * 1024 {
            format!("{:.1}M", self.size as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1}G", self.size as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }*/
}

/// Contains metadata about a specific snapshot file
///
/// Used for operations like diffing and restoration, containing
/// the actual file content and additional metadata.
#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
    /// Basic info about the snapshot
    pub info: SnapshotInfo,
    /// Content of the snapshot file
    pub content: Vec<u8>,
    /// SHA-256 hash of the original file path, used for storage organization
    pub path_hash: String,
}

// Export the service implementation
pub mod service;
pub use service::SnapshotService;
