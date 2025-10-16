use std::sync::Arc;

use sha2::{Digest, Sha256};
use tracing::debug;

use crate::{Content, FsReadService};

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

    /// Detects files that have changed since the last recorded hash
    ///
    /// Iterates through all tracked files, recomputes the SHA-256 hash of each
    /// file's current content, and compares it with the stored hash.
    /// Returns a list of file paths that have different hashes.
    ///
    /// # Arguments
    ///
    /// * `tracked_files` - Map of file paths to their stored hashes
    pub async fn detect(
        &self,
        tracked_files: &std::collections::HashMap<String, String>,
    ) -> Vec<std::path::PathBuf> {
        let mut changes = Vec::new();

        for (path, stored_hash) in tracked_files {
            let file_path = std::path::PathBuf::from(path);

            // Read the current file content and compute its hash
            match self.read_file_content(&file_path).await {
                Ok(content) => {
                    let current_hash = compute_content_hash(&content);

                    // Compare with stored hash
                    if current_hash != *stored_hash {
                        debug!(
                            path = %path,
                            stored_hash = %stored_hash,
                            current_hash = %current_hash,
                            "Detected file modification"
                        );
                        changes.push(file_path);
                    }
                }
                Err(e) => {
                    // File has changed (deleted or inaccessible)
                    debug!(
                        path = %path,
                        error = ?e,
                        "File has changed or is inaccessible"
                    );
                    changes.push(file_path);
                }
            }
        }

        changes
    }

    /// Reads file content using the FsReadService
    async fn read_file_content(&self, path: &std::path::PathBuf) -> anyhow::Result<String> {
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
        error_files: HashMap<String, String>,
    }

    impl MockFsReadService {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
                not_found_files: Vec::new(),
                error_files: HashMap::new(),
            }
        }

        fn with_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
            self.files.insert(path.into(), content.into());
            self
        }

        fn with_not_found(mut self, path: impl Into<String>) -> Self {
            self.not_found_files.push(path.into());
            self
        }

        fn with_error(mut self, path: impl Into<String>, error_msg: impl Into<String>) -> Self {
            self.error_files.insert(path.into(), error_msg.into());
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

            if let Some(error_msg) = self.error_files.get(&path) {
                return Err(anyhow::anyhow!("{}", error_msg));
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
    async fn test_file_change_detector_no_changes() {
        let content = "hello world";
        let hash = compute_content_hash(content);
        let fixture = MockFsReadService::new().with_file("/test/file.txt", content);
        let detector = FileChangeDetector::new(Arc::new(fixture));
        let mut tracked_files = HashMap::new();
        tracked_files.insert("/test/file.txt".to_string(), hash);

        let actual = detector.detect(&tracked_files).await;
        let expected: Vec<std::path::PathBuf> = vec![];

        assert_eq!(actual.len(), expected.len());
    }

    #[tokio::test]
    async fn test_file_change_detector_modified_file() {
        let original_content = "hello world";
        let original_hash = compute_content_hash(original_content);
        let new_content = "hello world modified";
        let fixture = MockFsReadService::new().with_file("/test/file.txt", new_content);
        let detector = FileChangeDetector::new(Arc::new(fixture));
        let mut tracked_files = HashMap::new();
        tracked_files.insert("/test/file.txt".to_string(), original_hash);

        let actual = detector.detect(&tracked_files).await;

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0], std::path::PathBuf::from("/test/file.txt"));
    }

    #[tokio::test]
    async fn test_file_change_detector_deleted_file() {
        let original_hash = compute_content_hash("some content");
        let fixture = MockFsReadService::new().with_not_found("/test/file.txt");
        let detector = FileChangeDetector::new(Arc::new(fixture));
        let mut tracked_files = HashMap::new();
        tracked_files.insert("/test/file.txt".to_string(), original_hash);

        let actual = detector.detect(&tracked_files).await;

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0], std::path::PathBuf::from("/test/file.txt"));
    }

    #[tokio::test]
    async fn test_file_change_detector_unknown_error() {
        let original_hash = compute_content_hash("some content");
        let fixture = MockFsReadService::new().with_error("/test/file.txt", "permission denied");
        let detector = FileChangeDetector::new(Arc::new(fixture));
        let mut tracked_files = HashMap::new();
        tracked_files.insert("/test/file.txt".to_string(), original_hash);

        let actual = detector.detect(&tracked_files).await;

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0], std::path::PathBuf::from("/test/file.txt"));
    }

    #[tokio::test]
    async fn test_file_change_detector_multiple_files_mixed_changes() {
        let content1 = "content1";
        let content2 = "content2";
        let hash1 = compute_content_hash(content1);
        let hash2 = compute_content_hash(content2);
        let hash3 = compute_content_hash("original3");

        let fixture = MockFsReadService::new()
            .with_file("/test/file1.txt", content1) // unchanged
            .with_file("/test/file2.txt", "modified content2") // modified
            .with_not_found("/test/file3.txt"); // deleted

        let detector = FileChangeDetector::new(Arc::new(fixture));
        let mut tracked_files = HashMap::new();
        tracked_files.insert("/test/file1.txt".to_string(), hash1);
        tracked_files.insert("/test/file2.txt".to_string(), hash2);
        tracked_files.insert("/test/file3.txt".to_string(), hash3);

        let actual = detector.detect(&tracked_files).await;
        assert_eq!(actual.len(), 2);

        // Check that both changed files are present
        assert!(actual.contains(&std::path::PathBuf::from("/test/file2.txt")));
        assert!(actual.contains(&std::path::PathBuf::from("/test/file3.txt")));
    }

    #[tokio::test]
    async fn test_file_change_detector_empty_tracked_files() {
        let fixture = MockFsReadService::new();
        let detector = FileChangeDetector::new(Arc::new(fixture));
        let tracked_files = HashMap::new();

        let actual = detector.detect(&tracked_files).await;
        let expected: Vec<std::path::PathBuf> = vec![];

        assert_eq!(actual.len(), expected.len());
    }
}
