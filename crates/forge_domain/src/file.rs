use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub is_dir: bool,
}

/// Information about a file or file range read operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    /// Starting line position of the read operation
    pub start_line: u64,

    /// Ending line position of the read operation
    pub end_line: u64,

    /// Total number of lines in the file
    pub total_lines: u64,

    /// SHA-256 hash of the **full** file content.
    /// Stored so callers have a stable hash that matches what a subsequent
    /// whole-file read produces (used by the external-change detector).
    pub content_hash: String,
}

impl FileInfo {
    /// Creates a new FileInfo with the specified parameters.
    pub fn new(start_line: u64, end_line: u64, total_lines: u64, content_hash: String) -> Self {
        Self { start_line, end_line, total_lines, content_hash }
    }

    /// Returns true if this represents a partial file read
    pub fn is_partial(&self) -> bool {
        self.start_line > 0 || self.end_line < self.total_lines
    }
}

/// File hash information from the server
///
/// Contains the relative file path and its SHA-256 hash
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHash {
    /// Relative file path from workspace root
    pub path: String,
    /// SHA-256 hash of the file content
    pub hash: String,
}

impl From<super::node::FileNode> for FileHash {
    fn from(node: super::node::FileNode) -> Self {
        Self { path: node.file_path, hash: node.hash }
    }
}

/// Status of a file in relation to the server
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum SyncStatus {
    /// File is in sync with server (same hash)
    InSync,
    /// File has been modified locally
    Modified,
    /// File is new (not on server)
    New,
    /// File exists on server but not locally (deleted locally)
    Deleted,
    /// File could not be read locally (e.g. permission error, binary file)
    Failed,
}

/// Information about a file's sync status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStatus {
    /// Relative file path from workspace root
    pub path: String,
    /// Sync status of the file
    pub status: SyncStatus,
}

impl FileStatus {
    /// Create a new file status entry
    pub fn new(path: String, status: SyncStatus) -> Self {
        Self { path, status }
    }
}
