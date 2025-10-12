use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_app::{Content, FsReadService, ReadOutput};
use strum_macros::{Display, EnumString};

use crate::range::resolve_range;
use crate::utils::assert_absolute_path;
use crate::{EnvironmentInfra, FileInfoInfra, FileReaderInfra as InfraFsReadService};

/// Supported image formats for binary file reading
#[derive(Debug, Clone, Copy, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
enum ImageFormat {
    #[strum(serialize = "jpg", serialize = "jpeg")]
    Jpeg,
    Png,
    Webp,
    Gif,
}

impl ImageFormat {
    /// Returns the MIME type for this image format
    fn mime_type(&self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Gif => "image/gif",
        }
    }

    /// Returns a comma-separated list of supported formats
    fn supported_formats() -> &'static str {
        "JPEG, PNG, WebP, GIF"
    }
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

        // Validate file size before reading content
        assert_file_size(&*self.0, path, env.max_file_size).await?;

        let (start_line, end_line) = resolve_range(start_line, end_line, env.max_read_size);

        let (content, file_info) = self
            .0
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

    async fn read_binary(&self, path: String) -> anyhow::Result<Content> {
        let path = Path::new(&path);
        assert_absolute_path(path)?;
        let env = self.0.get_environment();

        // Validate file size before reading content using binary file size limit
        assert_file_size(&*self.0, path, env.max_file_size).await?;

        // Determine image format from file extension
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| {
                anyhow::anyhow!("File has no extension. Cannot determine image format.")
            })?;

        let format = extension.parse::<ImageFormat>().map_err(|_| {
            anyhow::anyhow!(
                "Unsupported image format: {}. Supported formats: {}",
                extension,
                ImageFormat::supported_formats()
            )
        })?;

        // Read the binary content
        let content = self
            .0
            .read(path)
            .await
            .with_context(|| format!("Failed to read binary file from {}", path.display()))?;

        let image = forge_app::domain::Image::new_bytes(content, format.mime_type());

        Ok(Content::image(image))
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

    // Tests for read_binary
    use std::path::PathBuf;

    use forge_app::FsReadService;

    use crate::attachment::tests::MockCompositeService;

    #[tokio::test]
    async fn test_read_binary_png() {
        let infra = Arc::new(MockCompositeService::new());
        infra.add_file(
            PathBuf::from("/test/image.png"),
            "fake-png-content".to_string(),
        );

        let service = ForgeFsRead::new(infra);
        let actual = service.read_binary("/test/image.png".to_string()).await;

        assert!(actual.is_ok());
        let content = actual.unwrap();
        match content {
            Content::Image(image) => {
                assert_eq!(image.mime_type(), "image/png");
            }
            _ => panic!("Expected Image content"),
        }
    }

    #[tokio::test]
    async fn test_read_binary_jpeg() {
        let infra = Arc::new(MockCompositeService::new());
        infra.add_file(
            PathBuf::from("/test/photo.jpg"),
            "fake-jpeg-content".to_string(),
        );

        let service = ForgeFsRead::new(infra);
        let actual = service.read_binary("/test/photo.jpg".to_string()).await;

        assert!(actual.is_ok());
        let content = actual.unwrap();
        match content {
            Content::Image(image) => {
                assert_eq!(image.mime_type(), "image/jpeg");
            }
            _ => panic!("Expected Image content"),
        }
    }

    #[tokio::test]
    async fn test_read_binary_jpeg_alternate_extension() {
        let infra = Arc::new(MockCompositeService::new());
        infra.add_file(
            PathBuf::from("/test/photo.jpeg"),
            "fake-jpeg-content".to_string(),
        );

        let service = ForgeFsRead::new(infra);
        let actual = service.read_binary("/test/photo.jpeg".to_string()).await;

        assert!(actual.is_ok());
        let content = actual.unwrap();
        match content {
            Content::Image(image) => {
                assert_eq!(image.mime_type(), "image/jpeg");
            }
            _ => panic!("Expected Image content"),
        }
    }

    #[tokio::test]
    async fn test_read_binary_webp() {
        let infra = Arc::new(MockCompositeService::new());
        infra.add_file(
            PathBuf::from("/test/image.webp"),
            "fake-webp-content".to_string(),
        );

        let service = ForgeFsRead::new(infra);
        let actual = service.read_binary("/test/image.webp".to_string()).await;

        assert!(actual.is_ok());
        let content = actual.unwrap();
        match content {
            Content::Image(image) => {
                assert_eq!(image.mime_type(), "image/webp");
            }
            _ => panic!("Expected Image content"),
        }
    }

    #[tokio::test]
    async fn test_read_binary_gif() {
        let infra = Arc::new(MockCompositeService::new());
        infra.add_file(
            PathBuf::from("/test/animation.gif"),
            "fake-gif-content".to_string(),
        );

        let service = ForgeFsRead::new(infra);
        let actual = service.read_binary("/test/animation.gif".to_string()).await;

        assert!(actual.is_ok());
        let content = actual.unwrap();
        match content {
            Content::Image(image) => {
                assert_eq!(image.mime_type(), "image/gif");
            }
            _ => panic!("Expected Image content"),
        }
    }

    #[tokio::test]
    async fn test_read_binary_unsupported_format() {
        let infra = Arc::new(MockCompositeService::new());
        infra.add_file(
            PathBuf::from("/test/document.pdf"),
            "fake-pdf-content".to_string(),
        );

        let service = ForgeFsRead::new(infra);
        let actual = service.read_binary("/test/document.pdf".to_string()).await;

        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("Unsupported image format")
        );
    }

    #[tokio::test]
    async fn test_read_binary_no_extension() {
        let infra = Arc::new(MockCompositeService::new());
        infra.add_file(PathBuf::from("/test/noext"), "fake-content".to_string());

        let service = ForgeFsRead::new(infra);
        let actual = service.read_binary("/test/noext".to_string()).await;

        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("File has no extension")
        );
    }

    #[tokio::test]
    async fn test_read_binary_nonexistent_file() {
        let infra = Arc::new(MockCompositeService::new());
        let service = ForgeFsRead::new(infra);
        let actual = service
            .read_binary("/test/nonexistent.png".to_string())
            .await;

        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_read_binary_base64_encoding() {
        let infra = Arc::new(MockCompositeService::new());
        let test_content = "test-image-bytes";
        infra.add_file(PathBuf::from("/test/encode.png"), test_content.to_string());

        let service = ForgeFsRead::new(infra);
        let actual = service.read_binary("/test/encode.png".to_string()).await;

        assert!(actual.is_ok());
        let content = actual.unwrap();

        match content {
            Content::Image(image) => {
                // Verify the image URL contains base64 encoded data
                assert!(image.url().starts_with("data:image/png;base64,"));

                // Verify we can decode the base64 data back
                use base64::Engine;
                let expected_base64 =
                    base64::engine::general_purpose::STANDARD.encode(test_content);
                assert!(image.url().ends_with(&expected_base64));
            }
            _ => panic!("Expected Image content"),
        }
    }

    #[tokio::test]
    async fn test_read_binary_size_limit() {
        let infra = Arc::new(MockCompositeService::new());
        // Create a file larger than default max_binary_file_size (256kb)
        let large_content = "x".repeat((256 << 10) + 1); // 2 MB
        infra.add_file(PathBuf::from("/test/large.png"), large_content);

        let service = ForgeFsRead::new(infra);
        let actual = service.read_binary("/test/large.png".to_string()).await;

        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("exceeds the maximum allowed size")
        );
    }
}
