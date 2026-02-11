use std::sync::Arc;

use forge_domain::Metrics;
use tracing::debug;

use crate::FsReadService;

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
                    // Get current hash from the full raw file content (not the
                    // truncated/formatted content returned to the LLM).
                    // ReadOutput.content_hash is always computed from the
                    // unprocessed file, so it is directly comparable with the
                    // stored hash.
                    let current_hash = self.read_file_hash(&file_path).await.ok();

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

    /// Reads a file and returns its content hash computed from the full raw
    /// content, bypassing any line truncation or range limiting.
    async fn read_file_hash(&self, path: &std::path::Path) -> anyhow::Result<String> {
        let output = self
            .fs_read_service
            .read(path.to_string_lossy().to_string(), None, None)
            .await?;

        Ok(output.content_hash)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_domain::{FileOperation, Metrics, ToolKind};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::Content;
    use crate::utils::compute_hash;

    /// Mock FsReadService for testing.
    ///
    /// Returns `content_hash` computed from the raw content (mirroring
    /// the real implementation at `fs_read.rs:164`), while `content`
    /// may differ (simulating truncation / formatting).
    struct MockFsReadService {
        files: HashMap<String, MockFile>,
        not_found_files: Vec<String>,
    }

    struct MockFile {
        /// The raw, unprocessed file content (used to compute content_hash)
        raw_content: String,
        /// The content returned in ReadOutput.content (may be truncated)
        displayed_content: String,
    }

    impl MockFsReadService {
        fn new() -> Self {
            Self { files: HashMap::new(), not_found_files: Vec::new() }
        }

        /// Adds a file where displayed content equals raw content (no
        /// truncation).
        fn with_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
            let content = content.into();
            self.files.insert(
                path.into(),
                MockFile { raw_content: content.clone(), displayed_content: content },
            );
            self
        }

        /// Adds a file where the displayed content differs from the raw
        /// content, simulating line truncation or range limiting.
        fn with_truncated_file(
            mut self,
            path: impl Into<String>,
            raw_content: impl Into<String>,
            displayed_content: impl Into<String>,
        ) -> Self {
            self.files.insert(
                path.into(),
                MockFile {
                    raw_content: raw_content.into(),
                    displayed_content: displayed_content.into(),
                },
            );
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

            if let Some(file) = self.files.get(&path) {
                Ok(crate::ReadOutput {
                    content: Content::File(file.displayed_content.clone()),
                    start_line: 1,
                    end_line: 1,
                    total_lines: 1,
                    content_hash: compute_hash(&file.raw_content),
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

    #[tokio::test]
    async fn test_read_file_with_matching_hash_not_detected() {
        let content = "hello world";
        let content_hash = compute_hash(content);

        let fs = MockFsReadService::new().with_file("/test/file.txt", content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        let mut metrics = Metrics::default();
        metrics.file_operations.insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Read).content_hash(Some(content_hash)),
        );

        // Hash computed from raw content matches stored hash -- no change
        let actual = detector.detect(&metrics).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_truncated_content_does_not_cause_false_positive() {
        // Simulates a file with a very long line that gets truncated when
        // displayed, but the content_hash is still computed from raw content.
        let raw_content = "a".repeat(5000); // long line that would be truncated
        let displayed_content = "a".repeat(2000); // truncated version
        let raw_hash = compute_hash(&raw_content);

        let fs = MockFsReadService::new().with_truncated_file(
            "/test/file.txt",
            &raw_content,
            &displayed_content,
        );
        let detector = FileChangeDetector::new(Arc::new(fs));

        let mut metrics = Metrics::default();
        metrics.file_operations.insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Read).content_hash(Some(raw_hash)),
        );

        // Even though displayed content differs from raw, the hash comparison
        // uses the raw-based content_hash from ReadOutput, so no false positive.
        let actual = detector.detect(&metrics).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_truncated_written_file_not_false_positive() {
        // Same scenario but for a written file -- ensures the fix applies to
        // all ToolKinds, not just Read.
        let raw_content = "line1\n".repeat(3000); // > 2000 lines, would be range-limited
        let displayed_content = "line1\n".repeat(2000); // truncated version
        let raw_hash = compute_hash(&raw_content);

        let fs = MockFsReadService::new().with_truncated_file(
            "/test/file.txt",
            &raw_content,
            &displayed_content,
        );
        let detector = FileChangeDetector::new(Arc::new(fs));

        let mut metrics = Metrics::default();
        metrics.file_operations.insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Write).content_hash(Some(raw_hash)),
        );

        // Hash from ReadOutput.content_hash (raw) matches stored hash -- no
        // false positive despite displayed content being truncated.
        let actual = detector.detect(&metrics).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_read_then_write_same_file_no_external_change() {
        // Simulates: agent reads file, then writes to it, file is unchanged
        // on disk since the write.
        let original = "original content";
        let written = "written content";
        let written_hash = compute_hash(written);

        let fs = MockFsReadService::new().with_file("/test/file.txt", written);
        let detector = FileChangeDetector::new(Arc::new(fs));

        // Step 1: Read the file (insert via Metrics::insert like production)
        let metrics = Metrics::default().insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Read).content_hash(Some(compute_hash(original))),
        );
        // Step 2: Write the file (overwrites the Read entry)
        let metrics = metrics.insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Write).content_hash(Some(written_hash)),
        );

        // File on disk matches what was written -- no change
        let actual = detector.detect(&metrics).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_read_then_write_same_file_externally_modified() {
        // Simulates: agent reads file, writes to it, then user modifies
        // the file externally.
        let written = "written content";
        let external = "user modified this";
        let written_hash = compute_hash(written);
        let external_hash = compute_hash(external);

        // Disk now has the externally modified content
        let fs = MockFsReadService::new().with_file("/test/file.txt", external);
        let detector = FileChangeDetector::new(Arc::new(fs));

        // Step 1: Read, Step 2: Write
        let metrics = Metrics::default()
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Read).content_hash(Some(compute_hash("original"))),
            )
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Write).content_hash(Some(written_hash)),
            );

        // External modification detected
        let actual = detector.detect(&metrics).await;
        let expected = vec![FileChange {
            path: std::path::PathBuf::from("/test/file.txt"),
            content_hash: Some(external_hash),
        }];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_write_then_read_back_same_file_no_false_positive() {
        // Simulates: agent writes file, then reads it back. The last
        // operation in file_operations is Read, but it should still
        // not report a false positive since the hash matches.
        let content = "final content";
        let content_hash = compute_hash(content);

        let fs = MockFsReadService::new().with_file("/test/file.txt", content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        // Step 1: Write, Step 2: Read back (overwrites Write entry)
        let metrics = Metrics::default()
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Write).content_hash(Some(content_hash.clone())),
            )
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Read).content_hash(Some(content_hash)),
            );

        // Last entry is Read with matching hash -- no false positive
        let actual = detector.detect(&metrics).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_mixed_read_and_write_multiple_files() {
        // Simulates a real workflow:
        //   - Read file A (inspect)
        //   - Read file B (inspect)
        //   - Write file B (modify)
        //   - Patch file C
        //   - Read file D (inspect only)
        // Then user externally modifies file B.
        let a_content = "file a content";
        let b_written = "file b written";
        let b_external = "file b external edit";
        let c_content = "file c patched";
        let d_content = "file d content";

        let fs = MockFsReadService::new()
            .with_file("/test/a.txt", a_content)
            .with_file("/test/b.txt", b_external) // user modified B
            .with_file("/test/c.txt", c_content)
            .with_file("/test/d.txt", d_content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        let metrics = Metrics::default()
            .insert(
                "/test/a.txt".to_string(),
                FileOperation::new(ToolKind::Read).content_hash(Some(compute_hash(a_content))),
            )
            .insert(
                "/test/b.txt".to_string(),
                FileOperation::new(ToolKind::Read)
                    .content_hash(Some(compute_hash("file b original"))),
            )
            .insert(
                "/test/b.txt".to_string(),
                FileOperation::new(ToolKind::Write).content_hash(Some(compute_hash(b_written))),
            )
            .insert(
                "/test/c.txt".to_string(),
                FileOperation::new(ToolKind::Patch).content_hash(Some(compute_hash(c_content))),
            )
            .insert(
                "/test/d.txt".to_string(),
                FileOperation::new(ToolKind::Read).content_hash(Some(compute_hash(d_content))),
            );

        let actual = detector.detect(&metrics).await;

        // Only file B should be detected: externally modified after write.
        // A and D are unchanged. C is unchanged.
        let expected = vec![FileChange {
            path: std::path::PathBuf::from("/test/b.txt"),
            content_hash: Some(compute_hash(b_external)),
        }];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_read_only_file_externally_modified_still_detected() {
        // If a file was only read, and then externally modified, detect()
        // SHOULD still report it -- the purpose is to notify the LLM that
        // context it saw is now stale.
        let original = "original";
        let modified = "someone changed this";

        let fs = MockFsReadService::new().with_file("/test/file.txt", modified);
        let detector = FileChangeDetector::new(Arc::new(fs));

        let metrics = Metrics::default().insert(
            "/test/file.txt".to_string(),
            FileOperation::new(ToolKind::Read).content_hash(Some(compute_hash(original))),
        );

        let actual = detector.detect(&metrics).await;
        let expected = vec![FileChange {
            path: std::path::PathBuf::from("/test/file.txt"),
            content_hash: Some(compute_hash(modified)),
        }];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_multiple_patches_then_detect_no_change() {
        // Agent patches a file multiple times, disk still matches
        // the final patch.
        let final_content = "v3";
        let final_hash = compute_hash(final_content);

        let fs = MockFsReadService::new().with_file("/test/file.txt", final_content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        let metrics = Metrics::default()
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Read).content_hash(Some(compute_hash("v0"))),
            )
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Patch).content_hash(Some(compute_hash("v1"))),
            )
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Patch).content_hash(Some(compute_hash("v2"))),
            )
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Patch).content_hash(Some(final_hash)),
            );

        let actual = detector.detect(&metrics).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_write_then_undo_then_detect() {
        // Agent writes a file, then undoes it. The undo operation records
        // the restored content hash. Disk should match.
        let original = "original";
        let original_hash = compute_hash(original);

        let fs = MockFsReadService::new().with_file("/test/file.txt", original);
        let detector = FileChangeDetector::new(Arc::new(fs));

        let metrics = Metrics::default()
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Read).content_hash(Some(original_hash.clone())),
            )
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Write).content_hash(Some(compute_hash("modified"))),
            )
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Undo).content_hash(Some(original_hash)),
            );

        let actual = detector.detect(&metrics).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_truncated_read_then_write_no_false_positive() {
        // Read a file with long lines (content gets truncated in display),
        // then write to it. The write hash should match disk.
        let raw_content = "a".repeat(5000);
        let written_content = "new short content";
        let written_hash = compute_hash(written_content);

        // After write, disk has the written content
        let fs = MockFsReadService::new().with_file("/test/file.txt", written_content);
        let detector = FileChangeDetector::new(Arc::new(fs));

        // Read (truncated display), then Write
        let metrics = Metrics::default()
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Read).content_hash(Some(compute_hash(&raw_content))),
            )
            .insert(
                "/test/file.txt".to_string(),
                FileOperation::new(ToolKind::Write).content_hash(Some(written_hash)),
            );

        let actual = detector.detect(&metrics).await;
        let expected = vec![];

        assert_eq!(actual, expected);
    }
}
