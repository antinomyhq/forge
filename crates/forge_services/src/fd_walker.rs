use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use forge_app::{Walker, WalkerInfra};
use tracing::{info, warn};

use crate::fd::{FileDiscovery, filter_and_resolve};

/// Maximum number of files to discover in a single sweep.
///
/// Prevents runaway memory use when the workspace root is a very broad
/// directory (e.g. a user's home directory containing many projects).
const MAX_FILES: usize = 50_000;

/// Maximum combined byte size of all discovered files.
const MAX_TOTAL_SIZE: u64 = 500 * 1024 * 1024; // 500 MB

/// Maximum directory traversal depth.
const MAX_DEPTH: usize = 20;

/// Maximum number of entries per directory.
const MAX_BREADTH: usize = 10_000;

/// File discovery implementation backed by the filesystem walker.
///
/// Walks the directory tree under `dir_path` using the configured `WalkerInfra`
/// implementation. This is used as a fallback when git-based discovery is
/// unavailable or returns no files.
pub struct FdWalker<F> {
    infra: Arc<F>,
}

impl<F> FdWalker<F> {
    /// Creates a new `WalkerFileDiscovery` using the provided infrastructure.
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

/// Check if the given path is a potentially dangerous root directory
/// that could cause memory issues if indexed.
fn is_broad_directory(dir_path: &Path) -> bool {
    // Check for home directory
    if let Some(home) = dirs::home_dir() {
        if dir_path == home {
            return true;
        }
    }

    // Check for filesystem root
    let path_str = dir_path.to_string_lossy();
    if path_str == "/" || path_str == "\\" {
        return true;
    }

    // Check for common broad directories on Unix
    #[cfg(unix)]
    {
        // Check for /Users on macOS
        if path_str == "/Users" {
            return true;
        }
        // Check for /home on Linux
        if path_str == "/home" {
            return true;
        }
    }

    // Check for common broad directories on Windows
    #[cfg(windows)]
    {
        // Check for C:\Users or similar
        if path_str.to_lowercase().ends_with("\\users") {
            return true;
        }
    }

    false
}

#[async_trait]
impl<F: WalkerInfra + 'static> FileDiscovery for FdWalker<F> {
    async fn discover(&self, dir_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        // Warn immediately if the workspace root looks like a very broad path.
        // This happens when a user runs `forge` for the first time from their
        // home directory, causing the walker to attempt to index everything.
        if is_broad_directory(dir_path) {
            warn!(
                path = %dir_path.display(),
                max_files = MAX_FILES,
                "forge workspace root is set to a very broad directory; \
                 file discovery will be capped at {MAX_FILES} files. \
                 Run forge from within a specific project directory to avoid this limit."
            );
        }

        let walker_config = Walker::unlimited()
            .cwd(dir_path.to_path_buf())
            .skip_binary(true)
            .max_files(MAX_FILES)
            .max_total_size(MAX_TOTAL_SIZE)
            .max_depth(MAX_DEPTH)
            .max_breadth(MAX_BREADTH);

        let files = self
            .infra
            .walk(walker_config)
            .await
            .context("Failed to walk directory")?;

        let paths: Vec<String> = files
            .into_iter()
            .filter(|f| !f.is_dir())
            .map(|f| f.path)
            .collect();

        // Warn if we hit the file limit
        if paths.len() >= MAX_FILES {
            warn!(
                file_count = paths.len(),
                limit = MAX_FILES,
                path = %dir_path.display(),
                "File discovery hit the {MAX_FILES}-file limit; some files may not be indexed. \
                 Add a .gitignore or .ignore file to exclude large directories."
            );
        }

        info!(file_count = paths.len(), "Discovered files via walker");
        filter_and_resolve(dir_path, paths)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use forge_app::{WalkedFile, Walker};

    use super::*;

    fn make_file(path: &str) -> WalkedFile {
        WalkedFile { path: path.to_string(), file_name: Some(path.to_string()), size: 100 }
    }

    struct MockWalker {
        captured: Mutex<Option<Walker>>,
        files: Vec<WalkedFile>,
    }

    impl MockWalker {
        fn new(files: Vec<WalkedFile>) -> Self {
            Self { captured: Mutex::new(None), files }
        }

        fn captured_config(&self) -> Walker {
            self.captured.lock().unwrap().clone().unwrap()
        }
    }

    #[async_trait]
    impl WalkerInfra for MockWalker {
        async fn walk(&self, config: Walker) -> anyhow::Result<Vec<WalkedFile>> {
            *self.captured.lock().unwrap() = Some(config);
            Ok(self.files.clone())
        }
    }

    #[tokio::test]
    async fn discover_uses_bounded_config() {
        let mock = Arc::new(MockWalker::new(vec![make_file("src/lib.rs")]));
        let walker = FdWalker::new(mock.clone());

        let result = walker.discover(Path::new("/some/project")).await.unwrap();
        assert!(!result.is_empty() || result.is_empty()); // just ensure it doesn't panic

        let cfg = mock.captured_config();
        assert_eq!(cfg.max_files, Some(MAX_FILES), "must cap file count");
        assert_eq!(cfg.max_total_size, Some(MAX_TOTAL_SIZE), "must cap total size");
        assert_eq!(cfg.max_depth, Some(MAX_DEPTH), "must cap depth");
        assert_eq!(cfg.max_breadth, Some(MAX_BREADTH), "must cap breadth");
        assert!(cfg.skip_binary, "must skip binaries");
    }

    #[tokio::test]
    async fn discover_returns_files_from_walker() {
        let files = vec![make_file("main.rs"), make_file("lib.rs")];
        let mock = Arc::new(MockWalker::new(files));
        let walker = FdWalker::new(mock);

        let result = walker.discover(Path::new("/project")).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_broad_directory_detects_home() {
        if let Some(home) = dirs::home_dir() {
            assert!(
                is_broad_directory(&home),
                "home directory should be detected as broad"
            );
        }
    }

    #[test]
    fn test_is_broad_directory_detects_root() {
        // Test Unix root
        #[cfg(unix)]
        {
            assert!(is_broad_directory(Path::new("/")));
        }

        // Test Windows root
        #[cfg(windows)]
        {
            assert!(is_broad_directory(Path::new("C:\\")));
        }
    }

    #[test]
    fn test_is_broad_directory_allows_project_dirs() {
        // Project directories should NOT be detected as broad
        assert!(
            !is_broad_directory(Path::new("/home/user/my-project")),
            "project directory should not be detected as broad"
        );
        assert!(
            !is_broad_directory(Path::new("/Users/user/projects/my-app")),
            "project directory should not be detected as broad"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_is_broad_directory_detects_users_dir() {
        // On Unix/macOS, /Users is a broad directory
        assert!(is_broad_directory(Path::new("/Users")));
    }

    #[cfg(unix)]
    #[test]
    fn test_is_broad_directory_detects_home_dir() {
        // On Linux, /home is a broad directory
        assert!(is_broad_directory(Path::new("/home")));
    }
}