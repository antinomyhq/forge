use std::cmp;
use std::path::Path;

use anyhow::{Context, Result};
use forge_domain::FileInfo;

use crate::error::Error;

impl crate::ForgeFS {
    /// Reads a specific range of lines from a file.
    ///
    /// # Arguments
    /// * `path` - Path to the file to read
    /// * `start_line` - Starting line number (1-based, inclusive)
    /// * `end_line` - Ending line number (1-based, inclusive)
    ///
    /// Returns a tuple containing:
    /// - The file content as a UTF-8 string.
    /// - FileInfo containing metadata about the read operation including line
    ///   positions.
    pub async fn read_range_utf8<T: AsRef<Path>>(
        path: T,
        start_line: u64,
        end_line: u64,
    ) -> Result<(String, FileInfo)> {
        let path_ref = path.as_ref();

        // Basic validation
        if start_line > end_line {
            return Err(Error::StartGreaterThanEnd { start: start_line, end: end_line }.into());
        }

        // Open and check if file is binary
        let mut file = tokio::fs::File::open(path_ref)
            .await
            .with_context(|| format!("Failed to open file {}", path_ref.display()))?;

        if start_line == 0 || end_line == 0 {
            return Err(Error::IndexStartingWithZero { start: start_line, end: end_line }.into());
        }

        let (is_text, file_type) = Self::is_binary(&mut file).await?;
        if !is_text {
            return Err(Error::BinaryFileNotSupported(file_type).into());
        }

        // Read file content
        let content = tokio::fs::read(path_ref)
            .await
            .with_context(|| format!("Failed to read file content from {}", path_ref.display()))?;
        let content = String::from_utf8_lossy(&content);
        if start_line < 2 && content.is_empty() {
            // If the file is empty, return empty content
            return Ok((String::new(), FileInfo::new(start_line, end_line, 0)));
        }
        // Split into lines
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len() as u64;

        // Convert to 0-based indexing
        let start_pos = start_line.saturating_sub(1);
        let mut end_pos = end_line.saturating_sub(1);

        // Validate start position
        if start_pos >= total_lines {
            return Err(
                Error::StartBeyondFileSize { start: start_line, total: total_lines }.into(),
            );
        }

        // Cap end position at last line
        end_pos = cmp::min(end_pos, total_lines - 1);

        // Calculate actual end line (1-based) that was used
        let actual_end_line = cmp::min(end_line, total_lines);

        let info = FileInfo::new(start_line, actual_end_line, total_lines);

        // Extract requested lines
        let result_content = if start_pos == 0 && end_pos == total_lines - 1 {
            content.to_string() // Return full content if requesting entire file
        } else {
            lines[start_pos as usize..=end_pos as usize].join("\n")
        };

        Ok((result_content, info))
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use pretty_assertions::assert_eq;
    use tokio::fs;

    // Helper to create a temporary file with test content
    async fn create_test_file(content: &str) -> Result<tempfile::NamedTempFile> {
        let file = tempfile::NamedTempFile::new()?;
        fs::write(file.path(), content).await?;
        Ok(file)
    }

    #[tokio::test]
    async fn test_read_range_utf8() -> Result<()> {
        let content =
            "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10";
        let file = create_test_file(content).await?;

        // Test reading a range of lines
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 2, 5).await?;
        assert_eq!(result, "Line 2\nLine 3\nLine 4\nLine 5");
        assert_eq!(info.start_line, 2);
        assert_eq!(info.end_line, 5);
        assert_eq!(info.total_lines, 10);

        // Test reading from start
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 1, 3).await?;
        assert_eq!(result, "Line 1\nLine 2\nLine 3");
        assert_eq!(info.start_line, 1);
        assert_eq!(info.end_line, 3);

        // Test reading to end
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 8, 10).await?;
        assert_eq!(result, "Line 8\nLine 9\nLine 10");
        assert_eq!(info.start_line, 8);
        assert_eq!(info.end_line, 10);

        // Test reading entire file
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 1, 10).await?;
        assert_eq!(result, content);
        assert_eq!(info.start_line, 1);
        assert_eq!(info.end_line, 10);

        // Test single line
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 5, 5).await?;
        assert_eq!(result, "Line 5");
        assert_eq!(info.start_line, 5);
        assert_eq!(info.end_line, 5);

        // Test first line specifically
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 1, 1).await?;
        assert_eq!(result, "Line 1");
        assert_eq!(info.start_line, 1);
        assert_eq!(info.end_line, 1);
        assert_eq!(info.total_lines, 10);

        // Test invalid ranges
        assert!(
            crate::ForgeFS::read_range_utf8(file.path(), 8, 5)
                .await
                .is_err()
        );
        assert!(
            crate::ForgeFS::read_range_utf8(file.path(), 15, 10)
                .await
                .is_err()
        );
        assert!(
            crate::ForgeFS::read_range_utf8(file.path(), 0, 5)
                .await
                .is_err()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_utf8_multi_line_handling() -> Result<()> {
        let content = "Hello world!\nこんにちは 世界!\nПривет мир!\nBonjour le monde!";
        let file = create_test_file(content).await?;

        // Test reading a range that includes multi-byte characters
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 2, 3).await?;
        assert_eq!(result, "こんにちは 世界!\nПривет мир!");
        assert_eq!(info.start_line, 2);
        assert_eq!(info.end_line, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_utf8_handling() -> Result<()> {
        let content = b"Hello world!\nValid line\n\xFF\xFE\xFD Invalid UTF-8\nAnother valid line";
        let file = tempfile::NamedTempFile::new()?;
        fs::write(file.path(), content).await?;

        // Attempt to read the file shouldn't fail with invalid UTF-8 error
        let result = crate::ForgeFS::read_range_utf8(&file.path(), 1, 4).await;
        assert!(result.is_ok());
        Ok(())
    }


    #[tokio::test]
    async fn test_end_line_capped_at_total_lines() -> Result<()> {
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let file = create_test_file(content).await?;

        // Request end_line beyond file size
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 1, 100).await?;
        
        // Should read all lines
        assert_eq!(result, content);
        assert_eq!(info.start_line, 1);
        // end_line should be capped at total_lines, not the requested 100
        assert_eq!(info.end_line, 5);
        assert_eq!(info.total_lines, 5);

        // Test with range starting in the middle
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 3, 100).await?;
        assert_eq!(result, "Line 3\nLine 4\nLine 5");
        assert_eq!(info.start_line, 3);
        assert_eq!(info.end_line, 5);
        assert_eq!(info.total_lines, 5);

        Ok(())
    }
}
