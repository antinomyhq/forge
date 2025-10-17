use std::collections::HashMap;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tracing::debug;

use crate::{Content, FsReadService};

/// Information about a detected file change
#[derive(Debug, Clone, PartialEq)]
pub struct FileChange {
    pub path: std::path::PathBuf,
    /// File hash if readable, None if unreadable
    pub file_hash: Option<String>,
}

/// Detects file changes by comparing current file hashes with stored hashes
#[derive(Clone)]
pub struct FileChangeDetector<F> {
    fs_read_service: Arc<F>,
}

impl<F: FsReadService> FileChangeDetector<F> {
    /// Creates a new FileChangeDetector with the provided file read service
    ///
    /// # Arguments
    ///
    /// * `fs_read_service` - The file system read service implementation
    pub fn new(fs_read_service: Arc<F>) -> Self {
        Self { fs_read_service }
    }

    /// Detects files that have changed since the last notification
    ///
    /// Compares current file hash with stored hash. Returns a list of file
    /// changes.
    ///
    /// # Arguments
    ///
    /// * `tracked_files` - Map of file paths to their last known hashes (None
    ///   if unreadable)
    pub async fn detect(&self, tracked_files: &HashMap<String, Option<String>>) -> Vec<FileChange> {
        let mut changes = Vec::new();

        for (path, last_hash) in tracked_files {
            let file_path = std::path::PathBuf::from(path);

            // Get current hash: Some(hash) if readable, None if unreadable
            let current_hash = match self.read_file_content(&file_path).await {
                Ok(content) => Some(compute_content_hash(&content)),
                Err(_) => None,
            };

            // Check if hash has changed
            if current_hash != *last_hash {
                debug!(
                    path = %path,
                    last_hash = ?last_hash,
                    current_hash = ?current_hash,
                    "Detected file change"
                );
                changes.push(FileChange { path: file_path, file_hash: current_hash });
            }
        }

        changes
    }

    /// Reads file content using the FsReadService
    async fn read_file_content(&self, path: &std::path::Path) -> anyhow::Result<String> {
        let output = self
            .fs_read_service
            .read(path.to_string_lossy().to_string(), None, None)
            .await?;

        match output.content {
            Content::File(content) => Ok(content),
        }
    }
}

/// Computes SHA-256 hash of the given content
fn compute_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;

    use super::*;

    /// Mock FsReadService for testing
    struct MockFsReadService {
        files: HashMap<String, String>,
        not_found_files: Vec<String>,
    }

    impl MockFsReadService {
        fn new() -> Self {
            Self { files: HashMap::new(), not_found_files: Vec::new() }
        }

        fn with_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
            self.files.insert(path.into(), content.into());
            self
        }

        fn with_not_found(mut self, path: impl Into<String>) -> Self {
            self.not_found_files.push(path.into());
            self
        }
    }

    #[async_trait::async_trait]
    impl FsReadService for MockFsReadService {
        async fn read(
            &self,
            path: String,
            _: Option<u64>,
            _: Option<u64>,
        ) -> anyhow::Result<crate::ReadOutput> {
            if self.not_found_files.contains(&path) {
                return Err(anyhow::anyhow!(std::io::Error::from(
                    std::io::ErrorKind::NotFound
                )));
            }

            if let Some(content) = self.files.get(&path) {
                Ok(crate::ReadOutput {
                    content: Content::File(content.clone()),
                    start_line: 1,
                    end_line: 1,
                    total_lines: 1,
                })
            } else {
                Err(anyhow::anyhow!(std::io::Error::from(
                    std::io::ErrorKind::NotFound
                )))
            }
        }
    }

    #[tokio::test]
    async fn test_no_change() {
        let content = "hello world";
        let file_hash = compute_content_hash(content);

        let fs = MockFsReadService::new().with_file("/test/file.txt", content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        let mut tracked_files = HashMap::new();
        tracked_files.insert("/test/file.txt".to_string(), Some(file_hash));

        let actual = detector.detect(&tracked_files).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_file_modified() {
        let old_hash = compute_content_hash("old content");
        let new_content = "new content";
        let new_hash = compute_content_hash(new_content);

        let fs = MockFsReadService::new().with_file("/test/file.txt", new_content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        let mut tracked_files = HashMap::new();
        tracked_files.insert("/test/file.txt".to_string(), Some(old_hash));

        let actual = detector.detect(&tracked_files).await;
        let expected = vec![FileChange {
            path: std::path::PathBuf::from("/test/file.txt"),
            file_hash: Some(new_hash),
        }];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_file_becomes_unreadable() {
        let old_hash = compute_content_hash("old content");

        let fs = MockFsReadService::new().with_not_found("/test/file.txt");
        let detector = FileChangeDetector::new(Arc::new(fs));

        let mut tracked_files = HashMap::new();
        tracked_files.insert("/test/file.txt".to_string(), Some(old_hash));

        let actual = detector.detect(&tracked_files).await;
        let expected = vec![FileChange {
            path: std::path::PathBuf::from("/test/file.txt"),
            file_hash: None,
        }];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_no_duplicate_notification() {
        let new_content = "new content";
        let new_hash = compute_content_hash(new_content);
        let old_hash = "old_hash".to_string();

        let fs = MockFsReadService::new().with_file("/test/file.txt", new_content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        // First call: detect change
        let mut tracked_files = HashMap::new();
        tracked_files.insert("/test/file.txt".to_string(), Some(old_hash));

        let first = detector.detect(&tracked_files).await;
        assert_eq!(first.len(), 1);

        // Simulate updating file_hash after notification (like app.rs does)
        tracked_files.insert("/test/file.txt".to_string(), Some(new_hash));

        // Second call: should not detect change
        let actual = detector.detect(&tracked_files).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }
}
