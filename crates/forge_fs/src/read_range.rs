use std::path::Path;

use anyhow::{Context, Result};
use forge_domain::{FileInfo, LineRangeExt};

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
        let content = tokio::fs::read_to_string(path_ref)
            .await
            .with_context(|| format!("Failed to read file content from {}", path_ref.display()))?;

        // Extract the requested line range using the trait
        let range = content.extract(start_line, end_line)?;
        let info = FileInfo::new(range.start, range.end, range.total);

        Ok((range.content, info))
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
}
