use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub is_dir: bool,
}

/// Information about a file or file range read operation
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    /// Starting line position of the read operation
    pub start_line: u64,

    /// Ending line position of the read operation
    pub end_line: u64,

    /// Total number of lines in the file
    pub total_lines: u64,
}

impl FileInfo {
    /// Creates a new FileInfo with the specified parameters
    pub fn new(start_line: u64, end_line: u64, total_lines: u64) -> Self {
        Self { start_line, end_line, total_lines }
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

/// Status of a file in relation to the server
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum FileSyncStatus {
    /// File is in sync with server (same hash)
    InSync,
    /// File has been modified locally
    Modified,
    /// File is new (not on server)
    New,
    /// File exists on server but not locally (deleted locally)
    Deleted,
}

/// Information about a file's sync status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStatus {
    /// Relative file path from workspace root
    pub path: String,
    /// Sync status of the file
    pub status: FileSyncStatus,
}

impl FileStatus {
    /// Create a new file status entry
    pub fn new(path: String, status: FileSyncStatus) -> Self {
        Self { path, status }
    }
}
