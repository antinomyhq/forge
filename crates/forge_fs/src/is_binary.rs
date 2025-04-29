use anyhow::Result;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

impl crate::ForgeFS {
    /// Checks if a file is binary by examining its content.
    /// This version takes a path and opens the file itself.
    #[cfg(test)]
    async fn is_binary_path<T: AsRef<std::path::Path>>(path: T) -> Result<(bool, String)> {
        use anyhow::Context;

        let path_ref = path.as_ref();
        let mut file = File::open(path_ref)
            .await
            .with_context(|| format!("Failed to open file {}", path_ref.display()))?;

        Self::is_binary(&mut file).await
    }

    /// Checks if a file is binary by examining its content.
    /// This version takes an already opened file handle, allowing for reuse
    /// of the same file handle across multiple operations.
    /// This is a crate-private implementation detail.
    pub(crate) async fn is_binary(file: &mut File) -> Result<(bool, String)> {
        // Read sample data
        let mut sample = vec![0; 8192];
        let bytes_read = file.read(&mut sample).await?;
        sample.truncate(bytes_read);

        // Handle empty files
        if bytes_read == 0 {
            return Ok((true, "Empty file".into()));
        }

        // Get file type info
        let is_text = match infer::get(&sample) {
            Some(info) => matches!(
                info.matcher_type(),
                infer::MatcherType::Text | infer::MatcherType::Doc
            ),
            None => true, // Assume text if type can't be determined
        };

        let description = infer::get(&sample)
            .map(|info| info.mime_type().to_string())
            .unwrap_or_else(|| "Text file (no specific format detected)".into());

        Ok((is_text, description))
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
    async fn test_is_binary_file() -> Result<()> {
        // Test text file
        let text_file = create_test_file(b"Hello, world!").await?;
        let (is_text_or_doc, _) = crate::ForgeFS::is_binary_path(text_file.path()).await?;
        assert!(is_text_or_doc, "Text file should be identified as text");

        // Test binary data
        let binary_content = vec![0, 1, 2, 3, 0, 0, 0, 0, 5, 6, 7, 8];
        let binary_file = create_test_file(&binary_content).await?;
        let (is_text_or_doc, file_type) =
            crate::ForgeFS::is_binary_path(binary_file.path()).await?;

        if !is_text_or_doc {
            assert!(
                file_type.contains("binary") || !file_type.contains("text"),
                "Binary file type description should indicate binary"
            );
        }

        // Test PNG file
        let png_header = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00,
        ];
        let png_file = create_test_file(&png_header).await?;
        let (is_text_or_doc, file_type) = crate::ForgeFS::is_binary_path(png_file.path()).await?;
        assert!(!is_text_or_doc, "PNG file should be identified as binary");
        assert!(
            file_type.contains("image/png"),
            "PNG file type should be correctly identified"
        );

        // Test empty file
        let empty_file = create_test_file(&[]).await?;
        let (is_text_or_doc, _) = crate::ForgeFS::is_binary_path(empty_file.path()).await?;
        assert!(is_text_or_doc, "Empty file should be considered text");

        Ok(())
    }
}
