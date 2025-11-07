use std::cmp;
use std::path::Path;

use anyhow::{Context, Result};
use forge_domain::FileInfo;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::error::Error;
use crate::streaming::{FileAnalyzer, FileStrategy, LineSkipper, RangeReader};

/// Default maximum number of lines that can be read in a single request
#[allow(dead_code)]
pub const DEFAULT_MAX_LINES: u64 = 2000;

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
        max_lines_allowed: u64,
    ) -> Result<(String, FileInfo)> {
        let path_ref = path.as_ref();

        // Basic validation
        if start_line > end_line {
            return Err(Error::StartGreaterThanEnd { start: start_line, end: end_line }.into());
        }

        if start_line == 0 || end_line == 0 {
            return Err(Error::IndexStartingWithZero { start: start_line, end: end_line }.into());
        }

        // Open file
        let mut file = tokio::fs::File::open(path_ref)
            .await
            .with_context(|| format!("Failed to open file {}", path_ref.display()))?;

        // Check if file is binary
        let (is_text, file_type) = Self::is_binary(&mut file).await?;
        if !is_text {
            return Err(Error::BinaryFileNotSupported(file_type).into());
        }

        // Analyze file to determine optimal strategy
        let characteristics = FileAnalyzer::analyze_file_sample(path_ref).await?;
        let strategy = FileAnalyzer::determine_optimal_strategy(&characteristics);

        match strategy {
            FileStrategy::FullRead => {
                // Use existing logic for small files
                Self::read_full_file(&mut file, start_line, end_line).await
            }
            FileStrategy::OptimizedStream
            | FileStrategy::SafeStream
            | FileStrategy::Progressive => {
                // Use streaming for large files
                Self::read_streaming(
                    &mut file,
                    start_line,
                    end_line,
                    &characteristics,
                    max_lines_allowed,
                )
                .await
            }
        }
    }

    /// Reads small files using the traditional full-file approach
    async fn read_full_file(
        file: &mut tokio::fs::File,
        start_line: u64,
        end_line: u64,
    ) -> Result<(String, FileInfo)> {
        // Rewind to beginning and read entire file
        file.seek(std::io::SeekFrom::Start(0)).await?;
        let mut content = String::new();
        file.read_to_string(&mut content).await?;

        if content.is_empty() {
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

        // Extract requested lines
        let result_content = if start_pos == 0 && end_pos == total_lines - 1 {
            content // Return full content if requesting entire file
        } else {
            lines[start_pos as usize..=end_pos as usize].join("\n")
        };

        Ok((result_content, info))
    }

    /// Reads large files using optimized streaming approach
    async fn read_streaming(
        file: &mut tokio::fs::File,
        start_line: u64,
        end_line: u64,
        characteristics: &crate::streaming::FileCharacteristics,
        max_lines_allowed: u64,
    ) -> Result<(String, FileInfo)> {
        // Determine chunk size based on file characteristics
        let chunk_size = Self::determine_optimal_chunk_size(characteristics);
        let max_bytes_per_read = 1024 * 1024; // 1MB memory limit

        // Skip to the starting line
        let start_position = if start_line > 1 {
            LineSkipper::skip_to_line_optimized(file, start_line - 1, chunk_size).await?
        } else {
            0
        };

        // Adjust to exact line boundary
        let adjusted_start =
            LineSkipper::adjust_to_exact_line_boundary(file, start_position).await?;

        // Calculate how many lines we need to read
        let lines_to_read = cmp::min(end_line - start_line + 1, max_lines_allowed);

        // Read the content with memory limits
        let (content, actual_lines_read) = RangeReader::read_with_size_limit(
            file,
            adjusted_start,
            lines_to_read,
            max_bytes_per_read,
        )
        .await?;

        // Validate the result
        if !RangeReader::validate_line_count(
            &content,
            start_line,
            start_line + actual_lines_read - 1,
        ) {
            return Err(anyhow::anyhow!(
                "Line count validation failed. Expected {} lines, but got different count.",
                lines_to_read
            ));
        }

        // Estimate total lines in file (rough estimate based on characteristics)
        let total_lines = characteristics.line_count;

        let info = FileInfo::new(start_line, start_line + actual_lines_read - 1, total_lines);
        Ok((content, info))
    }

    /// Determines optimal chunk size based on file characteristics
    fn determine_chunk_size(file_size: u64) -> usize {
        match file_size {
            size if size < 100 * 1024 * 1024 => 64 * 1024, // 64KB for < 100MB
            size if size < 1024 * 1024 * 1024 => 128 * 1024, // 128KB for < 1GB
            _ => 256 * 1024,                               // 256KB for >= 1GB
        }
    }

    /// Determines optimal chunk size based on detailed file characteristics
    fn determine_optimal_chunk_size(
        characteristics: &crate::streaming::FileCharacteristics,
    ) -> usize {
        let base_size = Self::determine_chunk_size(characteristics.size_bytes);

        // Adjust based on line characteristics
        let adjustment_factor = if characteristics.avg_line_length > 200.0 {
            // Files with very long lines - use smaller chunks for better responsiveness
            0.5
        } else if characteristics.avg_line_length < 50.0 && characteristics.line_count > 100000 {
            // Files with many short lines - can use larger chunks
            1.5
        } else {
            1.0
        };

        // Additional adjustment for very long lines that might exceed memory limits
        let long_line_adjustment = if characteristics.max_line_length > 100_000 {
            // Files with extremely long lines (>100KB) - be very conservative
            0.25
        } else if characteristics.max_line_length > 10_000 {
            // Files with long lines (>10KB) - use smaller chunks
            0.75
        } else {
            1.0
        };

        // Final adjustment based on newline type (CRLF files are slightly larger)
        let newline_adjustment = match characteristics.newline_type {
            crate::streaming::NewlineType::Crlf => 0.9, // CRLF uses 2 bytes per newline
            crate::streaming::NewlineType::Mixed => 0.8, // Mixed format - be conservative
            crate::streaming::NewlineType::LF => 1.0,
            crate::streaming::NewlineType::CR => 0.95, // CR-only format - slightly conservative
        };

        (base_size as f64 * adjustment_factor * long_line_adjustment * newline_adjustment) as usize
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use pretty_assertions::assert_eq;
    use tokio::fs;

    use crate::read_range::DEFAULT_MAX_LINES;

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
        let (result, info) =
            crate::ForgeFS::read_range_utf8(file.path(), 2, 5, DEFAULT_MAX_LINES).await?;
        assert_eq!(result, "Line 2\nLine 3\nLine 4\nLine 5");
        assert_eq!(info.start_line, 2);
        assert_eq!(info.end_line, 5);
        assert_eq!(info.total_lines, 10);

        // Test reading from start
        let (result, info) =
            crate::ForgeFS::read_range_utf8(file.path(), 1, 3, DEFAULT_MAX_LINES).await?;
        assert_eq!(result, "Line 1\nLine 2\nLine 3");
        assert_eq!(info.start_line, 1);
        assert_eq!(info.end_line, 3);

        // Test reading to end
        let (result, info) =
            crate::ForgeFS::read_range_utf8(file.path(), 8, 10, DEFAULT_MAX_LINES).await?;
        assert_eq!(result, "Line 8\nLine 9\nLine 10");
        assert_eq!(info.start_line, 8);
        assert_eq!(info.end_line, 10);

        // Test reading entire file
        let (result, info) =
            crate::ForgeFS::read_range_utf8(file.path(), 1, 10, DEFAULT_MAX_LINES).await?;
        assert_eq!(result, content);
        assert_eq!(info.start_line, 1);
        assert_eq!(info.end_line, 10);

        // Test single line
        let (result, info) =
            crate::ForgeFS::read_range_utf8(file.path(), 5, 5, DEFAULT_MAX_LINES).await?;
        assert_eq!(result, "Line 5");
        assert_eq!(info.start_line, 5);
        assert_eq!(info.end_line, 5);

        // Test first line specifically
        let (result, info) =
            crate::ForgeFS::read_range_utf8(file.path(), 1, 1, DEFAULT_MAX_LINES).await?;
        assert_eq!(result, "Line 1");
        assert_eq!(info.start_line, 1);
        assert_eq!(info.end_line, 1);
        assert_eq!(info.total_lines, 10);

        // Test invalid ranges
        assert!(
            crate::ForgeFS::read_range_utf8(file.path(), 8, 5, DEFAULT_MAX_LINES)
                .await
                .is_err()
        );
        assert!(
            crate::ForgeFS::read_range_utf8(file.path(), 15, 10, DEFAULT_MAX_LINES)
                .await
                .is_err()
        );
        assert!(
            crate::ForgeFS::read_range_utf8(file.path(), 0, 5, DEFAULT_MAX_LINES)
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
        let (result, info) =
            crate::ForgeFS::read_range_utf8(file.path(), 2, 3, DEFAULT_MAX_LINES).await?;
        assert_eq!(result, "こんにちは 世界!\nПривет мир!");
        assert_eq!(info.start_line, 2);
        assert_eq!(info.end_line, 3);

        Ok(())
    }
}
