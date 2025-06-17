use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context};
use forge_app::{Content, EnvironmentService, FsReadService, ReadOutput};
use forge_domain::{Image, MimeType, Pdf};

use crate::utils::assert_absolute_path;
use crate::{FsMetaService, FsReadService as _, Infrastructure};

/// Resolves and validates line ranges, ensuring they are always valid
/// and within the specified maximum size.
///
/// # Arguments
/// * `start_line` - Optional starting line position (defaults to 1)
/// * `end_line` - Optional ending line position
/// * `max_size` - The maximum allowed range size
///
/// # Returns
/// A tuple of (start_line, end_line) that is guaranteed to be valid
///
/// # Behavior
/// - If start_line is None, defaults to 1
/// - If end_line is None, defaults to start_line + max_size
/// - If end_line < start_line, swaps them to ensure valid range
/// - If range exceeds max_size, adjusts end_line to stay within limits
/// - Always ensures start_line >= 1
pub fn resolve_range(start_line: Option<u64>, end_line: Option<u64>, max_size: u64) -> (u64, u64) {
    // 1. Normalise incoming values
    let s0 = start_line.unwrap_or(1).max(1);
    let e0 = end_line.unwrap_or(s0.saturating_add(max_size.saturating_sub(1)));

    // 2. Sort them (min → start, max → end) and force start ≥ 1
    let start = s0.min(e0).max(1);
    let mut end = s0.max(e0);

    // 3. Clamp the range length to `max_size`
    end = end.min(start.saturating_add(max_size - 1));

    (start, end)
}

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
async fn assert_file_size<F: Infrastructure>(
    infra: &F,
    path: &Path,
    max_file_size: u64,
) -> anyhow::Result<()> {
    let file_size = infra.file_meta_service().file_size(path).await?;
    if file_size > max_file_size {
        return Err(anyhow::anyhow!(
            "File size ({} bytes) exceeds the maximum allowed size of {} bytes",
            file_size,
            max_file_size
        ));
    }
    Ok(())
}

/// Reads file contents from the specified absolute path. Ideal for analyzing
/// code, configuration files, documentation, or textual data. Automatically
/// extracts text from PDF and DOCX files, preserving the original formatting.
/// Returns the content as a string. For files larger than 2,000 lines,
/// the tool automatically returns only the first 2,000 lines. You should
/// always rely on this default behavior and avoid specifying custom ranges
/// unless absolutely necessary. If needed, specify a range with the start_line
/// and end_line parameters, ensuring the total range does not exceed 2,000
/// lines. Specifying a range exceeding this limit will result in an error.
/// Binary files are automatically detected and rejected.
pub struct ForgeFsRead<F>(Arc<F>);

impl<F: Infrastructure> ForgeFsRead<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
    async fn read_utf8(
        &self,
        path: &Path,
        start_line: Option<u64>,
        end_line: Option<u64>,
    ) -> anyhow::Result<ReadOutput> {
        let env = self.0.environment_service().get_environment();

        // Validate file size before reading content
        assert_file_size(&*self.0, path, env.max_file_size).await?;

        let (start_line, end_line) = resolve_range(start_line, end_line, env.max_read_size);

        let (content, file_info) = self
            .0
            .file_read_service()
            .range_read_utf8(path, start_line, end_line)
            .await
            .with_context(|| format!("Failed to read file content from {}", path.display()))?;

        Ok(ReadOutput {
            content: Content::File(content),
            start_line: file_info.start_line,
            end_line: file_info.end_line,
            total_lines: file_info.total_lines,
        })
    }

    async fn read_raw(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        self.0
            .file_read_service()
            .read(path)
            .await
            .with_context(|| format!("Failed to read file content from {}", path.display()))
    }
    async fn read_pdf(&self, path: &Path, mime_type: MimeType) -> anyhow::Result<ReadOutput> {
        Ok(ReadOutput {
            content: Content::Pdf(Pdf::new_bytes(
                path.file_name().and_then(|v| v.to_str()).unwrap_or(""),
                &self.read_raw(path).await?,
                mime_type,
            )),
            start_line: 1,
            end_line: 1,
            total_lines: 1,
        })
    }

    async fn read_image(&self, path: &Path, mime_type: MimeType) -> anyhow::Result<ReadOutput> {
        Ok(ReadOutput {
            content: Content::Image(Image::new_bytes(&self.read_raw(path).await?, mime_type)),
            start_line: 1,
            end_line: 1,
            total_lines: 1,
        })
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> FsReadService for ForgeFsRead<F> {
    async fn read(
        &self,
        path: String,
        start_line: Option<u64>,
        end_line: Option<u64>,
    ) -> anyhow::Result<ReadOutput> {
        let path = Path::new(&path);
        assert_absolute_path(path)?;
        let ty = self.0.file_meta_service().mime_type(path).await?;
        match &ty {
            MimeType::Text => self.read_utf8(path, start_line, end_line).await,
            MimeType::Pdf => self.read_pdf(path, ty).await,
            MimeType::Image(_) => self.read_image(path, ty).await,
            MimeType::Other(_) => {
                bail!(
                    "Unsupported file type: {}. Only text and image files are supported.",
                    ty
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;
    use tokio::fs;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;

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
        let infra = MockInfrastructure::new();
        // Add the file to the mock infrastructure
        infra
            .file_service
            .add_file(fixture.path().to_path_buf(), "x".repeat(13));
        let actual = assert_file_size(&infra, fixture.path(), 20u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_assert_file_size_exactly_at_limit() {
        let fixture = create_test_file_with_size(6).await.unwrap();
        let infra = MockInfrastructure::new();
        infra
            .file_service
            .add_file(fixture.path().to_path_buf(), "x".repeat(6));
        let actual = assert_file_size(&infra, fixture.path(), 6u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_assert_file_size_exceeds_limit() {
        let fixture = create_test_file_with_size(45).await.unwrap();
        let infra = MockInfrastructure::new();
        infra
            .file_service
            .add_file(fixture.path().to_path_buf(), "x".repeat(45));
        let actual = assert_file_size(&infra, fixture.path(), 10u64).await;
        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_assert_file_size_empty_content() {
        let fixture = create_test_file_with_size(0).await.unwrap();
        let infra = MockInfrastructure::new();
        infra
            .file_service
            .add_file(fixture.path().to_path_buf(), "".to_string());
        let actual = assert_file_size(&infra, fixture.path(), 100u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_assert_file_size_zero_limit() {
        let fixture = create_test_file_with_size(1).await.unwrap();
        let infra = MockInfrastructure::new();
        infra
            .file_service
            .add_file(fixture.path().to_path_buf(), "x".to_string());
        let actual = assert_file_size(&infra, fixture.path(), 0u64).await;
        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_assert_file_size_large_content() {
        let fixture = create_test_file_with_size(1000).await.unwrap();
        let infra = MockInfrastructure::new();
        infra
            .file_service
            .add_file(fixture.path().to_path_buf(), "x".repeat(1000));
        let actual = assert_file_size(&infra, fixture.path(), 999u64).await;
        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_assert_file_size_large_content_within_limit() {
        let fixture = create_test_file_with_size(1000).await.unwrap();
        let infra = MockInfrastructure::new();
        infra
            .file_service
            .add_file(fixture.path().to_path_buf(), "x".repeat(1000));
        let actual = assert_file_size(&infra, fixture.path(), 1000u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_assert_file_size_unicode_content() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), "🚀🚀🚀").await.unwrap(); // Each emoji is 4 bytes in UTF-8 = 12 bytes total
        let infra = MockInfrastructure::new();
        infra
            .file_service
            .add_file(file.path().to_path_buf(), "🚀🚀🚀".to_string());
        let actual = assert_file_size(&infra, file.path(), 12u64).await;
        assert!(actual.is_ok());
    }

    #[tokio::test]
    async fn test_assert_file_size_unicode_content_exceeds() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), "🚀🚀🚀🚀").await.unwrap(); // 4 emojis = 16 bytes, exceeds 12 byte limit
        let infra = MockInfrastructure::new();
        infra
            .file_service
            .add_file(file.path().to_path_buf(), "🚀🚀🚀🚀".to_string());
        let actual = assert_file_size(&infra, file.path(), 12u64).await;
        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_assert_file_size_error_message() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), "too long content").await.unwrap(); // 16 bytes
        let infra = MockInfrastructure::new();
        infra
            .file_service
            .add_file(file.path().to_path_buf(), "too long content".to_string());
        let actual = assert_file_size(&infra, file.path(), 5u64).await;
        let expected = "File size (16 bytes) exceeds the maximum allowed size of 5 bytes";
        assert!(actual.is_err());
        assert_eq!(actual.unwrap_err().to_string(), expected);
    }

    #[test]
    fn test_resolve_range_with_defaults() {
        let fixture = (None, None, 100);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 100);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_start_only() {
        let fixture = (Some(5), None, 50);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (5, 54);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_both_start_and_end() {
        let fixture = (Some(10), Some(20), 100);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (10, 20);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_swapped_start_end() {
        let fixture = (Some(20), Some(10), 100);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (10, 20);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_exceeding_max_size() {
        let fixture = (Some(1), Some(200), 50);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 50);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_zero_start() {
        let fixture = (Some(0), Some(10), 20);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 10);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_zero_end_swapped() {
        let fixture = (Some(5), Some(0), 20);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 5);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_exact_max_size() {
        let fixture = (Some(1), Some(10), 10);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 10);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_max_size_boundary() {
        let fixture = (Some(5), Some(16), 10);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (5, 14);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_large_numbers() {
        let fixture = (Some(1000), Some(2000), 500);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1000, 1499);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_single_line() {
        let fixture = (Some(42), Some(42), 100);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (42, 42);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_end_only() {
        let fixture = (None, Some(50), 100);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 50);
        assert_eq!(actual, expected);
    }
}
