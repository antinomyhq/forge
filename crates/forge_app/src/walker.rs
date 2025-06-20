use std::path::PathBuf;

use derive_setters::Setters;

/// Configuration for filesystem walking operations
#[derive(Debug, Clone, Setters)]
#[setters(strip_option, into)]
pub struct Walker {
    /// Base directory to start walking from
    pub cwd: PathBuf,
    /// Maximum depth of directory traversal (None for unlimited)
    pub max_depth: Option<usize>,
    /// Maximum number of entries per directory (None for unlimited)
    pub max_breadth: Option<usize>,
    /// Maximum size of individual files to process (None for unlimited)
    pub max_file_size: Option<u64>,
    /// Maximum number of files to process in total (None for unlimited)
    pub max_files: Option<usize>,
    /// Maximum total size of all files combined (None for unlimited)
    pub max_total_size: Option<u64>,
    /// Whether to skip binary files
    pub skip_binary: bool,
}

impl Walker {
    /// Creates a new WalkerConfig with conservative default limits
    pub fn conservative() -> Self {
        Self {
            cwd: PathBuf::new(),
            max_depth: Some(5),
            max_breadth: Some(10),
            max_file_size: Some(1024 * 1024), // 1MB
            max_files: Some(100),
            max_total_size: Some(10 * 1024 * 1024), // 10MB
            skip_binary: true,
        }
    }

    /// Creates a new WalkerConfig with no limits (use with caution)
    pub fn unlimited() -> Self {
        Self {
            cwd: PathBuf::new(),
            max_depth: None,
            max_breadth: None,
            max_file_size: None,
            max_files: None,
            max_total_size: None,
            skip_binary: false,
        }
    }
}

impl Default for Walker {
    fn default() -> Self {
        Self::conservative()
    }
}

/// Represents a file or directory found during filesystem traversal
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalkedFile {
    /// Relative path from the base directory
    pub path: String,
    /// File name (None for root directory)
    pub file_name: Option<String>,
    /// Size in bytes
    pub size: u64,
}

impl WalkedFile {
    /// Returns true if this represents a directory
    pub fn is_dir(&self) -> bool {
        self.path.ends_with('/')
    }
}
