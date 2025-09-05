use std::cmp;
use std::path::Path;

use anyhow::{Context, Result};

use crate::error::Error;
use crate::file_info::FileInfo;

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

        let info = FileInfo::new(start_line, end_line, total_lines);

        // Calculate padding based on the maximum line number that will be displayed
        let max_line_number = if start_pos == 0 && end_pos == total_lines - 1 {
            total_lines
        } else {
            end_line
        };
        let padding = max_line_number.to_string().len();

        // Extract requested lines and add line numbers
        let result_lines: Vec<String> = if start_pos == 0 && end_pos == total_lines - 1 {
            // Full file requested - add line numbers to all lines
            lines
                .iter()
                .enumerate()
                .map(|(idx, line)| format!("{:>width$}: {}", idx + 1, line, width = padding))
                .collect()
        } else {
            // Range requested - add line numbers to the selected lines
            lines[start_pos as usize..=end_pos as usize]
                .iter()
                .enumerate()
                .map(|(idx, line)| {
                    format!(
                        "{:>width$}: {}",
                        start_pos + 1 + idx as u64,
                        line,
                        width = padding
                    )
                })
                .collect()
        };

        let result_content = result_lines.join("\n");

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
        assert_eq!(result, "2: Line 2\n3: Line 3\n4: Line 4\n5: Line 5");
        assert_eq!(info.start_line, 2);
        assert_eq!(info.end_line, 5);
        assert_eq!(info.total_lines, 10);

        // Test reading from start
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 1, 3).await?;
        assert_eq!(result, "1: Line 1\n2: Line 2\n3: Line 3");
        assert_eq!(info.start_line, 1);
        assert_eq!(info.end_line, 3);

        // Test reading to end
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 8, 10).await?;
        assert_eq!(result, " 8: Line 8\n 9: Line 9\n10: Line 10");
        assert_eq!(info.start_line, 8);
        assert_eq!(info.end_line, 10);

        // Test reading entire file
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 1, 10).await?;
        assert_eq!(
            result,
            " 1: Line 1\n 2: Line 2\n 3: Line 3\n 4: Line 4\n 5: Line 5\n 6: Line 6\n 7: Line 7\n 8: Line 8\n 9: Line 9\n10: Line 10"
        );
        assert_eq!(info.start_line, 1);
        assert_eq!(info.end_line, 10);

        // Test single line
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 5, 5).await?;
        assert_eq!(result, "5: Line 5");
        assert_eq!(info.start_line, 5);
        assert_eq!(info.end_line, 5);

        // Test first line specifically
        let (result, info) = crate::ForgeFS::read_range_utf8(file.path(), 1, 1).await?;
        assert_eq!(result, "1: Line 1");
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
        assert_eq!(result, "2: こんにちは 世界!\n3: Привет мир!");
        assert_eq!(info.start_line, 2);
        assert_eq!(info.end_line, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_dynamic_padding() -> Result<()> {
        // Create a file with 1000 lines to test different padding scenarios
        let lines: Vec<String> = (1..=1000).map(|i| format!("Line {}", i)).collect();
        let content = lines.join("\n");
        let file = create_test_file(&content).await?;

        // Test reading lines 1-9 (single digit): padding = 1
        let (result, _) = crate::ForgeFS::read_range_utf8(file.path(), 1, 9).await?;
        let first_line = result.lines().next().unwrap();
        assert_eq!(first_line, "1: Line 1");

        // Test reading lines 1-99 (double digit): padding = 2
        let (result, _) = crate::ForgeFS::read_range_utf8(file.path(), 1, 99).await?;
        let first_line = result.lines().next().unwrap();
        assert_eq!(first_line, " 1: Line 1");

        // Test reading lines 1-999 (triple digit): padding = 3
        let (result, _) = crate::ForgeFS::read_range_utf8(file.path(), 1, 999).await?;
        let first_line = result.lines().next().unwrap();
        assert_eq!(first_line, "  1: Line 1");

        // Test reading lines 1-1000 (quadruple digit): padding = 4
        let (result, _) = crate::ForgeFS::read_range_utf8(file.path(), 1, 1000).await?;
        let first_line = result.lines().next().unwrap();
        let last_line = result.lines().last().unwrap();
        assert_eq!(first_line, "   1: Line 1");
        assert_eq!(last_line, "1000: Line 1000");

        // Test reading a range in the middle with large numbers: lines 990-1000
        let (result, _) = crate::ForgeFS::read_range_utf8(file.path(), 990, 1000).await?;
        let first_line = result.lines().next().unwrap();
        let last_line = result.lines().last().unwrap();
        assert_eq!(first_line, " 990: Line 990");
        assert_eq!(last_line, "1000: Line 1000");

        Ok(())
    }
}
