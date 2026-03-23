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

    /// Creates a WalkerConfig suitable for workspace sync operations.
    ///
    /// Uses generous but bounded limits to prevent runaway memory consumption
    /// when the workspace root is a very large directory (e.g. a user's home
    /// directory). The limits are high enough for even large monorepos while
    /// still providing a hard ceiling on resource usage.
    pub fn sync() -> Self {
        Self {
            cwd: PathBuf::new(),
            max_depth: Some(20),
            max_breadth: Some(1_000),
            max_file_size: Some(10 * 1024 * 1024), // 10 MB
            max_files: Some(50_000),
            max_total_size: Some(500 * 1024 * 1024), // 500 MB
            skip_binary: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_preset_has_bounded_limits() {
        let walker = Walker::sync();
        assert!(walker.max_depth.is_some(), "sync preset must bound depth");
        assert!(
            walker.max_breadth.is_some(),
            "sync preset must bound breadth"
        );
        assert!(
            walker.max_file_size.is_some(),
            "sync preset must bound file size"
        );
        assert!(
            walker.max_files.is_some(),
            "sync preset must bound file count"
        );
        assert!(
            walker.max_total_size.is_some(),
            "sync preset must bound total size"
        );
        assert!(walker.skip_binary, "sync preset must skip binary files");
    }

    #[test]
    fn test_sync_preset_limits_are_generous() {
        let sync = Walker::sync();
        let conservative = Walker::conservative();

        assert!(
            sync.max_files.unwrap() > conservative.max_files.unwrap(),
            "sync preset should allow more files than conservative"
        );
        assert!(
            sync.max_depth.unwrap() > conservative.max_depth.unwrap(),
            "sync preset should allow more depth than conservative"
        );
        assert!(
            sync.max_total_size.unwrap() > conservative.max_total_size.unwrap(),
            "sync preset should allow more total size than conservative"
        );
    }

    #[test]
    fn test_unlimited_has_no_limits() {
        let walker = Walker::unlimited();
        assert!(walker.max_depth.is_none());
        assert!(walker.max_breadth.is_none());
        assert!(walker.max_file_size.is_none());
        assert!(walker.max_files.is_none());
        assert!(walker.max_total_size.is_none());
    }
}
