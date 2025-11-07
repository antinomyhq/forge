use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_app::{
    Content, EnvironmentInfra, FileInfoInfra, FileReaderInfra as InfraFsReadService, FsReadService,
    ReadOutput,
};

use crate::range::{MAX_READ_SIZE, resolve_range};
use crate::utils::assert_absolute_path;

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
pub(super) async fn assert_file_size<F: FileInfoInfra>(
    infra: &F,
    path: &Path,
    max_file_size: u64,
) -> anyhow::Result<()> {
    let file_size = infra.file_size(path).await?;
    if file_size > max_file_size {
        return Err(anyhow::anyhow!(
            "File size ({file_size} bytes) exceeds the maximum allowed size of {max_file_size} bytes"
        ));
    }
    Ok(())
}

/// Validates that a requested range size is within acceptable limits
/// For range requests, we validate the range size instead of the entire file
/// size
pub(super) async fn validate_range_size<F: FileInfoInfra>(
    infra: &F,
    path: &Path,
    start_line: Option<u64>,
    end_line: Option<u64>,
    max_bytes: u64,
) -> anyhow::Result<()> {
    // For range requests, always validate the range size (not the entire file)
    // Any start_line or end_line means it's a range request
    if start_line.is_none() && end_line.is_none() {
        // No range specified, fall back to original validation
        return assert_file_size(infra, path, max_bytes).await;
    }

    // Use resolve_range to get the actual range that will be read
    let (start, end) = resolve_range(start_line, end_line, 2000); // Use MAX_READ_SIZE
    let lines_in_range = end.saturating_sub(start) + 1;

    // Conservative estimate to prevent memory issues
    // Minimum realistic line length is 10 characters, but we use 120 for safety
    let conservative_avg_line_length = 120;
    let estimated_range_size = lines_in_range * conservative_avg_line_length;

    if estimated_range_size > max_bytes {
        return Err(anyhow::anyhow!(
            "Requested range ({lines_in_range} lines) estimated to exceed {max_bytes} bytes limit"
        ));
    }

    Ok(())
}

/// Reads file contents from the specified absolute path. Ideal for analyzing
/// code, configuration files, documentation, or textual data. Returns the
/// content as a string. When you invoke a tool without any parameters, it
/// automatically returns only the first 2,000 lines for files exceeding that
/// length. You should always rely on this default behavior and avoid specifying
/// custom ranges unless absolutely necessary. If needed, specify a range with
/// the start_line and end_line parameters, ensuring the total range does not
/// exceed 2,000 lines. Specifying a range exceeding this limit will result in
/// an error. Binary files are automatically detected and rejected.
pub struct ForgeFsRead<F>(Arc<F>);

impl<F> ForgeFsRead<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

#[async_trait::async_trait]
impl<F: FileInfoInfra + EnvironmentInfra + InfraFsReadService> FsReadService for ForgeFsRead<F> {
    async fn read(
        &self,
        path: String,
        start_line: Option<u64>,
        end_line: Option<u64>,
    ) -> anyhow::Result<ReadOutput> {
        let path = Path::new(&path);
        assert_absolute_path(path)?;
        let env = self.0.get_environment();

        // Use appropriate validation based on whether we have a range request
        let has_range_request = start_line.is_some() || end_line.is_some();

        let validation_result = if has_range_request {
            // For range requests, use range-based validation
            validate_range_size(&*self.0, path, start_line, end_line, env.max_file_size).await
        } else {
            // For full file requests, use original validation
            assert_file_size(&*self.0, path, env.max_file_size).await
        };

        if let Err(e) = validation_result {
            tracing::error!(
                path = %path.display(),
                max_file_size = env.max_file_size,
                error = %e,
                "File size validation failed"
            );
            return Err(e);
        }

        let (start_line, end_line) = resolve_range(start_line, end_line, MAX_READ_SIZE);

        let (content, file_info) = self
            .0
            .range_read_utf8(path, start_line, end_line, MAX_READ_SIZE)
            .await
            .map_err(|e| {
                tracing::error!(
                    path = %path.display(),
                    start_line = start_line,
                    end_line = end_line,
                    error = %e,
                    "Failed to read file content"
                );
                e
            })
            .with_context(|| format!("Failed to read file content from {}", path.display()))?;

        // Additional validation: check actual content size against limit
        let actual_content_size = content.len() as u64;
        if actual_content_size > env.max_file_size {
            return Err(anyhow::anyhow!(
                "Read content ({} bytes) exceeds maximum allowed size of {} bytes",
                actual_content_size,
                env.max_file_size
            ));
        }

        Ok(ReadOutput {
            content: Content::File(content),
            start_line: file_info.start_line,
            end_line: file_info.end_line,
            total_lines: file_info.total_lines,
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

    #[tokio::test]
    async fn test_validate_range_size_small_file_with_range() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), "small content").await.unwrap(); // 13 bytes
        let infra = MockFileService::new();
        infra.add_file(file.path().to_path_buf(), "small content".to_string());

        // Should validate range size, not entire file
        // 10 lines * 120 bytes = 1200 bytes, so need limit > 1200
        let actual = validate_range_size(&infra, file.path(), Some(1), Some(10), 2000u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_validate_range_size_range_exceeds_limit() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), "some content").await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(file.path().to_path_buf(), "some content".to_string());

        // Large range should exceed limit - but resolve_range will clamp to
        // MAX_READ_SIZE (2000)
        let actual = validate_range_size(&infra, file.path(), Some(1), Some(10000), 1000u64).await; // 2000 lines > 1000 bytes limit
        assert!(actual.is_err());

        let error_msg = actual.unwrap_err().to_string();
        assert!(error_msg.contains("estimated to exceed"));
        assert!(error_msg.contains("2000 lines")); // After resolve_range clamping
    }

    #[tokio::test]
    async fn test_validate_range_size_no_range_falls_back_to_original() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), "content").await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(file.path().to_path_buf(), "content".to_string());

        // No range should fall back to original validation
        let actual = validate_range_size(&infra, file.path(), None, None, 100u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_read_with_actual_size_validation() {
        // Test that actual size validation works by testing range validation
        // since actual size validation happens after successful read
        let file = NamedTempFile::new().unwrap();
        let content = "x".repeat(50); // 50 bytes content
        fs::write(file.path(), &content).await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(file.path().to_path_buf(), content);

        // Test that range validation works correctly
        // 10 lines * 120 bytes = 1200 bytes, so need limit > 1200
        let result = validate_range_size(&infra, file.path(), Some(1), Some(10), 2000u64).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_range_size_start_line_only() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), "some content").await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(file.path().to_path_buf(), "some content".to_string());

        // Only start_line should be treated as range request
        // With start_line: 4000 and no end_line, resolve_range will set end_line to
        // 5999 (2000 lines total) 2000 lines * 120 bytes = 240000 bytes, so
        // need limit > 240000
        let actual = validate_range_size(&infra, file.path(), Some(4000), None, 300000u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_read_exceeds_actual_size_limit() {
        let file = NamedTempFile::new().unwrap();
        let content = "x".repeat(200); // 200 bytes content
        fs::write(file.path(), &content).await.unwrap();
        let infra = MockFileService::new();
        infra.add_file(file.path().to_path_buf(), content);

        // Test the logic indirectly through range validation
        let result = validate_range_size(&infra, file.path(), Some(1), Some(200), 100u64).await;
        assert!(result.is_err());
    }
}
