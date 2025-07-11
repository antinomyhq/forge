/// A MIME type wrapper that provides type checking capabilities
#[derive(Debug, Clone, PartialEq)]
pub struct MimeType(infer::Type);

impl MimeType {
    /// Create a new MimeType from a string
    pub(crate) fn new(mime_type: infer::Type) -> Self {
        Self(mime_type)
    }

    /// Get the inner MIME type string
    pub fn as_str(&self) -> &str {
        self.0.mime_type()
    }

    /// Check if this MIME type represents a binary file
    pub fn is_binary(&self) -> bool {
        !self.is_text()
    }

    /// Check if this MIME type represents a text file
    pub fn is_text(&self) -> bool {
        matches!(
            self.0.matcher_type(),
            infer::MatcherType::Text | infer::MatcherType::Doc
        )
    }

    /// Check if this MIME type represents an image file
    pub fn is_image(&self) -> bool {
        matches!(self.0.matcher_type(), infer::MatcherType::Image)
    }
}

impl crate::ForgeFS {
    /// Detects the MIME type of a file by examining its content.
    /// This version takes a path and opens the file itself.
    pub async fn mime_type<T: AsRef<std::path::Path>>(path: T) -> anyhow::Result<Option<MimeType>> {
        use anyhow::Context;
        use tokio::fs::File;

        let path_ref = path.as_ref();
        let mut file = File::open(path_ref)
            .await
            .with_context(|| format!("Failed to open file {}", path_ref.display()))?;

        Self::mime_type_from_file(&mut file).await
    }

    /// Detects the MIME type of an already opened file by examining its
    /// content. This version takes an already opened file handle, allowing
    /// for reuse of the same file handle across multiple operations.
    pub async fn mime_type_from_file(
        file: &mut tokio::fs::File,
    ) -> anyhow::Result<Option<MimeType>> {
        use tokio::io::AsyncReadExt;

        // Read sample data
        let mut sample = vec![0; 8192];
        let bytes_read = file.read(&mut sample).await?;
        sample.truncate(bytes_read);

        // Handle empty files
        if bytes_read == 0 {
            return Ok(None);
        }

        // Get file type info
        Ok(infer::get(&sample).map(MimeType::new))
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use tokio::fs;

    async fn create_test_file(content: &[u8]) -> Result<tempfile::NamedTempFile> {
        let file = tempfile::NamedTempFile::new()?;
        fs::write(file.path(), content).await?;
        Ok(file)
    }

    #[tokio::test]
    async fn test_mime_type_detection() -> Result<()> {
        // Test PNG file
        let png_header = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00,
        ];
        let png_file = create_test_file(&png_header).await?;
        let mime_type = crate::ForgeFS::mime_type(png_file.path())
            .await?
            .expect("PNG should be detected");

        assert!(
            mime_type.is_binary(),
            "PNG file should be identified as binary"
        );
        assert!(
            mime_type.is_image(),
            "PNG file should be identified as image"
        );
        assert!(
            mime_type.as_str().contains("image/png"),
            "PNG file type should be correctly identified"
        );

        // Test empty file
        let empty_file = create_test_file(&[]).await?;
        let mime_type_opt = crate::ForgeFS::mime_type(empty_file.path()).await?;
        assert!(mime_type_opt.is_none(), "Empty file should return None");

        Ok(())
    }
}
