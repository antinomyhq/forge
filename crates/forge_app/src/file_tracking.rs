use std::sync::Arc;

use forge_domain::Metrics;
use tracing::debug;

use crate::utils::compute_hash;
use crate::{Content, FsReadService};

/// Information about a detected file change
#[derive(Debug, Clone, PartialEq)]
pub struct FileChange {
    pub path: std::path::PathBuf,
    /// File hash if readable, None if unreadable
    pub content_hash: Option<String>,
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
    /// changes sorted by path for deterministic ordering.
    ///
    /// # Arguments
    ///
    /// * `tracked_files` - Map of file paths to their last known hashes (None
    ///   if unreadable)
    pub async fn detect(&self, metrics: &Metrics) -> Vec<FileChange> {
        let futures: Vec<_> = metrics
            .file_operations
            .iter()
            .map(|(path, file_metrics)| {
                let file_path = std::path::PathBuf::from(path);
                let last_hash = file_metrics.content_hash.clone();

                async move {
                    // Get current hash: Some(hash) if readable, None if unreadable
                    let current_hash = match self.read_file_content(&file_path).await {
                        Ok(content) => Some(compute_hash(&content)),
                        Err(_) => None,
                    };

                    // Check if hash has changed
                    if current_hash != last_hash {
                        debug!(
                            path = %file_path.display(),
                            last_hash = ?last_hash,
                            current_hash = ?current_hash,
                            "Detected file change"
                        );
                        Some(FileChange { path: file_path, content_hash: current_hash })
                    } else {
                        None
                    }
                }
            })
            .collect();

        let mut changes: Vec<FileChange> = futures::future::join_all(futures)
            .await
            .into_iter()
            .flatten()
            .collect();

        // Sort by path for deterministic ordering
        changes.sort_by(|a, b| a.path.cmp(&b.path));

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
            Content::Image(_) => Err(anyhow::anyhow!("Cannot track changes for image files")),
            Content::Document(_) => Err(anyhow::anyhow!("Cannot track changes for document files")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_domain::{FileOperation, Metrics, ToolKind};
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
                    content_hash: compute_hash(content),
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
        let content_hash = compute_hash(content);

        let fs = MockFsReadService::new().with_file("/test/file.txt", content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        let mut metrics = Metrics::default();
        metrics.file_operations.insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Write).content_hash(Some(content_hash)),
        );

        let actual = detector.detect(&metrics).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_file_modified() {
        let old_hash = compute_hash("old content");
        let new_content = "new content";
        let new_hash = compute_hash(new_content);

        let fs = MockFsReadService::new().with_file("/test/file.txt", new_content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        let mut metrics = Metrics::default();
        metrics.file_operations.insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Write).content_hash(Some(old_hash)),
        );

        let actual = detector.detect(&metrics).await;
        let expected = vec![FileChange {
            path: std::path::PathBuf::from("/test/file.txt"),
            content_hash: Some(new_hash),
        }];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_file_becomes_unreadable() {
        let old_hash = compute_hash("old content");

        let fs = MockFsReadService::new().with_not_found("/test/file.txt");
        let detector = FileChangeDetector::new(Arc::new(fs));

        let mut metrics = Metrics::default();
        metrics.file_operations.insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Write).content_hash(Some(old_hash)),
        );

        let actual = detector.detect(&metrics).await;
        let expected = vec![FileChange {
            path: std::path::PathBuf::from("/test/file.txt"),
            content_hash: None,
        }];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_no_duplicate_notification() {
        let new_content = "new content";
        let new_hash = compute_hash(new_content);
        let old_hash = "old_hash".to_string();

        let fs = MockFsReadService::new().with_file("/test/file.txt", new_content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        // First call: detect change
        let mut metrics = Metrics::default();
        metrics.file_operations.insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Write).content_hash(Some(old_hash)),
        );

        let first = detector.detect(&metrics).await;
        assert_eq!(first.len(), 1);

        // Simulate updating content_hash after notification (like app.rs does)
        metrics.file_operations.insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Write).content_hash(Some(new_hash)),
        );

        // Second call: should not detect change
        let actual = detector.detect(&metrics).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }
}
