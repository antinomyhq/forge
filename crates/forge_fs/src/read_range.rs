use std::cmp;
use std::path::Path;

use anyhow::{Context, Result};

use crate::error::Error;
use crate::file_info::FileInfo;

impl crate::ForgeFS {
    /// Reads a specific range of characters from a file.
    ///
    /// Returns a tuple containing:
    /// - The file content as a UTF-8 string.
    /// - FileInfo containing metadata about the read operation including
    ///   character positions.
    pub async fn read_range_utf8<T: AsRef<Path>>(
        path: T,
        start_char: u64,
        end_char: u64,
    ) -> Result<(String, FileInfo)> {
        let path_ref = path.as_ref();

        // Open the file for binary check
        let mut file = tokio::fs::File::open(path_ref)
            .await
            .with_context(|| format!("Failed to open file {}", path_ref.display()))?;

        // Check if the file is binary
        let (is_text, file_type) = Self::is_binary(&mut file).await?;
        if !is_text {
            return Err(Error::BinaryFileNotSupported(file_type).into());
        }

        // Read the file content
        let content = tokio::fs::read_to_string(path_ref)
            .await
            .with_context(|| format!("Failed to read file content from {}", path_ref.display()))?;

        let total_chars = content.chars().count() as u64;

        // Validate and normalize the character range
        let (start_pos, end_pos) =
            Self::validate_char_range_bounds(total_chars, start_char, end_char)?;
        let info = FileInfo::new(start_pos, end_pos, total_chars);

        // Return empty result for empty ranges
        if start_pos == end_pos {
            return Ok((String::new(), info));
        }

        // Extract the requested character range
        let result_content = if start_pos == 0 && end_pos == total_chars {
            content // Return the full content if requesting the entire file
        } else {
            content
                .chars()
                .skip(start_pos as usize)
                .take((end_pos - start_pos) as usize)
                .collect()
        };

        Ok((result_content, info))
    }

    // Validate the requested range and ensure it falls within the file's character
    // count
    fn validate_char_range_bounds(
        total_chars: u64,
        start_pos: u64,
        end_pos: u64,
    ) -> Result<(u64, u64)> {
        // Check if start is beyond file size
        if start_pos > total_chars {
            return Err(Error::StartBeyondFileSize { start: start_pos, total: total_chars }.into());
        }

        // Cap end position at file size
        let end_pos = cmp::min(end_pos, total_chars);

        // Check if start is greater than end
        if start_pos > end_pos {
            return Err(Error::StartGreaterThanEnd { start: start_pos, end: end_pos }.into());
        }

        Ok((start_pos, end_pos))
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use pretty_assertions::assert_eq;
    use tokio::fs;

    // Helper to create a temporary file with test content.
    async fn create_test_file(content: &str) -> Result<tempfile::NamedTempFile> {
        let file = tempfile::NamedTempFile::new()?;
        fs::write(file.path(), content).await?;
        Ok(file)
    }

    #[tokio::test]
    async fn test_read_range_utf8() -> Result<()> {
        let content = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let file = create_test_file(content).await?;

        // Test reading a range of characters
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 10, 20).await?;

        assert_eq!(result, "ABCDEFGHIJ", "Range 10-20 should be ABCDEFGHIJ");
        assert_eq!(info.start_char, 10);
        assert_eq!(info.end_char, 20);
        assert_eq!(info.total_chars, content.len() as u64);

        // Test reading from start
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 0, 5).await?;

        assert_eq!(result, "01234", "Range 0-5 should be 01234");
        assert_eq!(info.start_char, 0);
        assert_eq!(info.end_char, 5);

        // Test reading to end
        let total_chars = content.chars().count() as u64;
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 50, total_chars).await?;

        assert_eq!(
            result, "opqrstuvwxyz",
            "Range 50-end should be opqrstuvwxyz"
        );
        assert_eq!(info.start_char, 50);
        assert_eq!(info.end_char, info.total_chars);

        // Test reading entire file
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 0, total_chars).await?;

        assert_eq!(
            result, content,
            "Reading entire file should match original content"
        );
        assert_eq!(info.start_char, 0);
        assert_eq!(info.end_char, info.total_chars);

        // Test empty range
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 10, 10).await?;

        assert_eq!(result, "", "Empty range should return empty string");
        assert_eq!(info.start_char, 10);
        assert_eq!(info.end_char, 10);

        // Test invalid ranges
        assert!(
            crate::ForgeFS::read_range_utf8(file.path(), 20, 10)
                .await
                .is_err(),
            "Start > end should error"
        );
        assert!(
            crate::ForgeFS::read_range_utf8(file.path(), 1000, total_chars)
                .await
                .is_err(),
            "Start beyond file size should error"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_utf8_boundary_handling() -> Result<()> {
        let content = "Hello 世界! こんにちは! Привет!";
        let file = create_test_file(content).await?;

        // Test reading a range that includes multi-byte characters
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 6, 8).await?;

        // Character-based indexing should handle multi-byte characters correctly
        assert_eq!(
            result, "世界",
            "Should read exactly the multi-byte characters"
        );
        assert_eq!(info.start_char, 6);
        assert_eq!(info.end_char, 8);

        Ok(())
    }
}
