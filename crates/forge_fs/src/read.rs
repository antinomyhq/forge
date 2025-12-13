use std::path::Path;

use anyhow::{Context, Result};

impl crate::ForgeFS {
    /// Reads a file as UTF-8 text.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read
    /// - The file contains invalid UTF-8 sequences
    /// - The file contains null bytes (0x00), which are not valid in UTF-8 text
    ///   and would cause issues when storing in databases
    pub async fn read_utf8<T: AsRef<Path>>(path: T) -> Result<String> {
        let path_ref = path.as_ref();
        let bytes = Self::read(path_ref).await?;

        // First check for null bytes which are not allowed in UTF-8 text
        // and cause issues with PostgreSQL and other databases
        if bytes.contains(&0) {
            anyhow::bail!(
                "File contains null bytes (0x00) and appears to be binary: {}",
                path_ref.display()
            );
        }

        // Now validate proper UTF-8 encoding
        String::from_utf8(bytes).with_context(|| {
            format!(
                "File contains invalid UTF-8 sequences: {}",
                path_ref.display()
            )
        })
    }

    pub async fn read<T: AsRef<Path>>(path: T) -> Result<Vec<u8>> {
        tokio::fs::read(path.as_ref())
            .await
            .with_context(|| format!("Failed to read file {}", path.as_ref().display()))
    }

    pub async fn read_to_string<T: AsRef<Path>>(path: T) -> Result<String> {
        tokio::fs::read_to_string(path.as_ref())
            .await
            .with_context(|| format!("Failed to read file as string {}", path.as_ref().display()))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;

    use super::*;

    async fn create_test_file_fixture(content: &[u8]) -> Result<NamedTempFile> {
        let file = NamedTempFile::new()?;
        tokio::fs::write(file.path(), content).await?;
        Ok(file)
    }

    #[tokio::test]
    async fn test_read_utf8_valid_text() {
        let fixture = create_test_file_fixture(b"Hello, world!").await.unwrap();
        let actual = crate::ForgeFS::read_utf8(fixture.path()).await.unwrap();
        let expected = "Hello, world!";
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_read_utf8_rejects_null_bytes() {
        let content = b"Hello\x00World";
        let fixture = create_test_file_fixture(content).await.unwrap();
        let actual = crate::ForgeFS::read_utf8(fixture.path()).await;

        assert!(actual.is_err());
        let error_msg = actual.unwrap_err().to_string();
        assert!(
            error_msg.contains("null bytes"),
            "Error should mention null bytes: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn test_read_utf8_rejects_invalid_utf8() {
        // Invalid UTF-8 sequence
        let content = vec![0xFF, 0xFE, 0xFD];
        let fixture = create_test_file_fixture(&content).await.unwrap();
        let actual = crate::ForgeFS::read_utf8(fixture.path()).await;

        assert!(actual.is_err());
        let error_msg = actual.unwrap_err().to_string();
        assert!(
            error_msg.contains("invalid UTF-8"),
            "Error should mention invalid UTF-8: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn test_read_utf8_empty_file() {
        let fixture = create_test_file_fixture(b"").await.unwrap();
        let actual = crate::ForgeFS::read_utf8(fixture.path()).await.unwrap();
        let expected = "";
        assert_eq!(actual, expected);
    }
}
