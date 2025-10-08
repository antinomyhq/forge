use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_app::{Content, FsReadService, ReadOutput};

use crate::range::resolve_range;
use crate::utils::assert_absolute_path;
use crate::{EnvironmentInfra, FileInfoInfra, FileReaderInfra as InfraFsReadService};

/// Validates that file size does not exceed the maximum allowed file size.
///
/// # Arguments
/// * `infra` - The infrastructure instance providing file metadata services
/// * `path` - The file path to check
/// * `max_file_size` - Maximum allowed file size in bytes
///
/// # Returns
/// * `Ok(())` if file size is within limits
/// * `Err(anyhow::Error)` if file exceeds max_file_size
async fn assert_file_size<F: FileInfoInfra>(
    infra: &F,
    path: &Path,
    max_file_size: u64,
) -> anyhow::Result<()> {
    let file_size = infra.file_size(path).await?;
    if file_size > max_file_size {
        return Err(anyhow::anyhow!(
            "File size ({} bytes) exceeds the maximum allowed size of {} bytes",
            file_size,
            max_file_size
        ));
    }
    Ok(())
}

/// Determines if a file has been modified externally by comparing current
/// content with snapshot
///
/// # Arguments
/// * `current` - Current file content as bytes
/// * `snapshot` - Optional snapshot content as bytes
///
/// # Returns
/// * `false` if snapshot is None (no snapshot = no external modification)
/// * `true` if snapshot exists and differs from current content
/// * `false` if snapshot exists and matches current content
fn has_external_modification(current: &[u8], snapshot: Option<&[u8]>) -> bool {
    match snapshot {
        None => false,
        Some(snap) => current != snap,
    }
}

/// Reads file contents from the specified absolute path. Ideal for analyzing
/// code, configuration files, documentation, or textual data. Returns the
/// content as a string. For files larger than 2,000 lines, the tool
/// automatically returns only the first 2,000 lines. You should always rely
/// on this default behavior and avoid specifying custom ranges unless
/// absolutely necessary. If needed, specify a range with the start_line and
/// end_line parameters, ensuring the total range does not exceed 2,000 lines.
/// Specifying a range exceeding this limit will result in an error. Binary
/// files are automatically detected and rejected.
pub struct ForgeFsRead<F>(Arc<F>);

impl<F> ForgeFsRead<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

#[async_trait::async_trait]
impl<F: FileInfoInfra + EnvironmentInfra + InfraFsReadService + crate::SnapshotInfra> FsReadService
    for ForgeFsRead<F>
{
    async fn read(
        &self,
        path: String,
        start_line: Option<u64>,
        end_line: Option<u64>,
    ) -> anyhow::Result<ReadOutput> {
        let path = Path::new(&path);
        assert_absolute_path(path)?;
        let env = self.0.get_environment();

        // Validate file size before reading content
        assert_file_size(&*self.0, path, env.max_file_size).await?;

        let (start_line, end_line) = resolve_range(start_line, end_line, env.max_read_size);

        // Read operation should happen only once
        let (content, file_info) = self
            .0
            .range_read_utf8(path, start_line, end_line)
            .await
            .with_context(|| format!("Failed to read file content from {}", path.display()))?;

        // Read full file content for comparison with snapshot

        let full_content = self.0.read(path).await?;

        // Retrieve latest snapshot content for modification detection
        let snapshot_content = self.0.get_latest_snapshot(path).await?;

        // Determine if file has been modified externally
        let externally_modified =
            has_external_modification(&full_content, snapshot_content.as_deref());

        Ok(ReadOutput {
            content: Content::File(content),
            start_line: file_info.start_line,
            end_line: file_info.end_line,
            total_lines: file_info.total_lines,
            externally_modified,
        })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;
    use tokio::fs;

    use super::*;
    use crate::attachment::tests::MockFileService;

    // Helper to create a temporary file with specific content size
    async fn create_test_file_with_size(size: usize) -> anyhow::Result<NamedTempFile> {
        let file = NamedTempFile::new()?;
        let content = "x".repeat(size);
        fs::write(file.path(), content).await?;
        Ok(file)
    }

    #[tokio::test]
    async fn test_assert_file_size_within_limit() {
        let fixture = create_test_file_with_size(13).await.unwrap();
        let infra = MockFileService::new();
        // Add the file to the mock infrastructure
        infra.add_file(fixture.path().to_path_buf(), "x".repeat(13));
        let actual = assert_file_size(&infra, fixture.path(), 20u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_assert_file_size_exactly_at_limit() {
        let fixture = create_test_file_with_size(6).await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(fixture.path().to_path_buf(), "x".repeat(6));
        let actual = assert_file_size(&infra, fixture.path(), 6u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_assert_file_size_exceeds_limit() {
        let fixture = create_test_file_with_size(45).await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(fixture.path().to_path_buf(), "x".repeat(45));
        let actual = assert_file_size(&infra, fixture.path(), 10u64).await;
        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_assert_file_size_empty_content() {
        let fixture = create_test_file_with_size(0).await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(fixture.path().to_path_buf(), "".to_string());
        let actual = assert_file_size(&infra, fixture.path(), 100u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_assert_file_size_zero_limit() {
        let fixture = create_test_file_with_size(1).await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(fixture.path().to_path_buf(), "x".to_string());
        let actual = assert_file_size(&infra, fixture.path(), 0u64).await;
        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_assert_file_size_large_content() {
        let fixture = create_test_file_with_size(1000).await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(fixture.path().to_path_buf(), "x".repeat(1000));
        let actual = assert_file_size(&infra, fixture.path(), 999u64).await;
        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_assert_file_size_large_content_within_limit() {
        let fixture = create_test_file_with_size(1000).await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(fixture.path().to_path_buf(), "x".repeat(1000));
        let actual = assert_file_size(&infra, fixture.path(), 1000u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_assert_file_size_unicode_content() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), "🚀🚀🚀").await.unwrap(); // Each emoji is 4 bytes in UTF-8 = 12 bytes total
        let infra = MockFileService::new();
        infra.add_file(file.path().to_path_buf(), "🚀🚀🚀".to_string());
        let actual = assert_file_size(&infra, file.path(), 12u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_assert_file_size_unicode_content_exceeds() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), "🚀🚀🚀🚀").await.unwrap(); // 4 emojis = 16 bytes, exceeds 12 byte limit
        let infra = MockFileService::new();
        infra.add_file(file.path().to_path_buf(), "🚀🚀🚀🚀".to_string());
        let actual = assert_file_size(&infra, file.path(), 12u64).await;
        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_assert_file_size_error_message() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), "too long content").await.unwrap(); // 16 bytes
        let infra = MockFileService::new();
        infra.add_file(file.path().to_path_buf(), "too long content".to_string());
        let actual = assert_file_size(&infra, file.path(), 5u64).await;
        let expected = "File size (16 bytes) exceeds the maximum allowed size of 5 bytes";
        assert!(actual.is_err());
        assert_eq!(actual.unwrap_err().to_string(), expected);
    }

    #[test]
    fn test_has_external_modification_none_snapshot() {
        let current = b"Hello, World!";
        let snapshot = None;

        let actual = has_external_modification(current, snapshot);
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_has_external_modification_identical_content() {
        let current = b"Hello, World!";
        let snapshot = Some(b"Hello, World!".as_slice());

        let actual = has_external_modification(current, snapshot);
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_has_external_modification_different_content() {
        let current = b"Hello, World!";
        let snapshot = Some(b"Goodbye, World!".as_slice());

        let actual = has_external_modification(current, snapshot);
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_has_external_modification_empty_files() {
        let current = b"";
        let snapshot = Some(b"".as_slice());

        let actual = has_external_modification(current, snapshot);
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_has_external_modification_unicode_content() {
        let current = "🚀 Hello, World! 🌍".as_bytes();
        let snapshot = Some("🚀 Hello, World! 🌍".as_bytes());

        let actual = has_external_modification(current, snapshot);
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_has_external_modification_unicode_different() {
        let current = "🚀 Hello, World! 🌍".as_bytes();
        let snapshot = Some("🌍 Goodbye, World! 🚀".as_bytes());

        let actual = has_external_modification(current, snapshot);
        let expected = true;

        assert_eq!(actual, expected);
    }
}
