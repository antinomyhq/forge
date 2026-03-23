use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use forge_app::{Walker, WalkerInfra};
use tracing::{info, warn};

use crate::fd::{FileDiscovery, filter_and_resolve};

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

#[async_trait]
impl<F: WalkerInfra + 'static> FileDiscovery for FdWalker<F> {
    async fn discover(&self, dir_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        let walker_config = Walker::sync().cwd(dir_path.to_path_buf());

        let max_files = walker_config.max_files;

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

        if let Some(limit) = max_files
            && paths.len() >= limit
        {
            warn!(
                limit,
                path = %dir_path.display(),
                "File discovery hit the limit; some files may not be indexed. \
                 Consider using a more specific workspace path or adding an \
                 .ignore file to exclude large directories."
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

    /// Mock `WalkerInfra` that captures the config it receives and returns
    /// a pre-configured list of files.
    struct MockWalkerInfra {
        captured_config: Mutex<Option<Walker>>,
        files_to_return: Vec<WalkedFile>,
    }

    impl MockWalkerInfra {
        fn new(files: Vec<WalkedFile>) -> Self {
            Self { captured_config: Mutex::new(None), files_to_return: files }
        }

        fn captured_config(&self) -> Walker {
            self.captured_config
                .lock()
                .unwrap()
                .clone()
                .expect("walk() was never called")
        }
    }

    #[async_trait]
    impl WalkerInfra for MockWalkerInfra {
        async fn walk(&self, config: Walker) -> anyhow::Result<Vec<WalkedFile>> {
            *self.captured_config.lock().unwrap() = Some(config);
            Ok(self.files_to_return.clone())
        }
    }

    fn make_walked_file(path: &str, size: u64) -> WalkedFile {
        let file_name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        WalkedFile { path: path.to_string(), file_name, size }
    }

    #[tokio::test]
    async fn test_discover_uses_bounded_sync_config() {
        let files = vec![make_walked_file("lib.rs", 100)];
        let mock = Arc::new(MockWalkerInfra::new(files));
        let fd = FdWalker::new(mock.clone());

        let dir = tempfile::tempdir().unwrap();
        // Result may error (no matching files on disk) — we only care about
        // the config that was passed to walk().
        let _ = fd.discover(dir.path()).await;

        let config = mock.captured_config();
        assert!(config.max_depth.is_some(), "sync config must bound depth");
        assert!(
            config.max_breadth.is_some(),
            "sync config must bound breadth"
        );
        assert!(
            config.max_file_size.is_some(),
            "sync config must bound file size"
        );
        assert!(
            config.max_files.is_some(),
            "sync config must bound file count"
        );
        assert!(
            config.max_total_size.is_some(),
            "sync config must bound total size"
        );
        assert!(config.skip_binary, "sync config must skip binary files");
    }

    #[tokio::test]
    async fn test_discover_sets_cwd_to_dir_path() {
        let files = vec![make_walked_file("main.rs", 50)];
        let mock = Arc::new(MockWalkerInfra::new(files));
        let fd = FdWalker::new(mock.clone());

        let dir = tempfile::tempdir().unwrap();
        let _ = fd.discover(dir.path()).await;

        let config = mock.captured_config();
        assert_eq!(config.cwd, dir.path().to_path_buf());
    }

    #[tokio::test]
    async fn test_discover_excludes_directories() {
        let files = vec![
            make_walked_file("src/", 0),        // directory — should be excluded
            make_walked_file("src/lib.rs", 80), // file with allowed extension
        ];
        let mock = Arc::new(MockWalkerInfra::new(files));
        let fd = FdWalker::new(mock);

        let dir = tempfile::tempdir().unwrap();
        let result = fd.discover(dir.path()).await.unwrap();

        // Only the .rs file should survive; the directory entry should be gone.
        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("lib.rs"));
    }

    #[tokio::test]
    async fn test_discover_resolves_paths_to_absolute() {
        let files = vec![make_walked_file("app.rs", 120)];
        let mock = Arc::new(MockWalkerInfra::new(files));
        let fd = FdWalker::new(mock);

        let dir = tempfile::tempdir().unwrap();
        let result = fd.discover(dir.path()).await.unwrap();

        for path in &result {
            assert!(
                path.is_absolute(),
                "discovered path must be absolute: {path:?}"
            );
        }
        assert_eq!(result[0], dir.path().join("app.rs"));
    }

    #[tokio::test]
    async fn test_discover_filters_non_source_extensions() {
        let files = vec![
            make_walked_file("image.png", 500),  // not an allowed extension
            make_walked_file("binary.exe", 500), // not an allowed extension
            make_walked_file("code.rs", 100),    // allowed
        ];
        let mock = Arc::new(MockWalkerInfra::new(files));
        let fd = FdWalker::new(mock);

        let dir = tempfile::tempdir().unwrap();
        let result = fd.discover(dir.path()).await.unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("code.rs"));
    }

    #[tokio::test]
    async fn test_discover_returns_error_when_no_source_files() {
        // All files have disallowed extensions — filter_and_resolve should error
        let files = vec![make_walked_file("photo.png", 200)];
        let mock = Arc::new(MockWalkerInfra::new(files));
        let fd = FdWalker::new(mock);

        let dir = tempfile::tempdir().unwrap();
        let result = fd.discover(dir.path()).await;

        assert!(result.is_err(), "should error when no source files found");
    }

    #[tokio::test]
    async fn test_discover_at_file_limit_still_returns_results() {
        // When the walker returns exactly max_files non-directory entries,
        // discover() should still return successfully (the warning is logged
        // but doesn't affect the result).
        let sync_config = Walker::sync();
        let limit = sync_config.max_files.unwrap();

        let files: Vec<WalkedFile> = (0..limit)
            .map(|i| make_walked_file(&format!("f{i}.rs"), 10))
            .collect();

        let mock = Arc::new(MockWalkerInfra::new(files));
        let fd = FdWalker::new(mock);

        let dir = tempfile::tempdir().unwrap();
        let result = fd.discover(dir.path()).await.unwrap();

        assert_eq!(result.len(), limit);
    }
}
