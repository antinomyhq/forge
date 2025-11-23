use std::path::Path;

use anyhow::{Context, Result};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Strategy for reading files based on their characteristics
#[derive(Debug, Clone, PartialEq)]
pub enum FileStrategy {
    /// Full file read for small files (< 10MB)
    FullRead,
    /// Optimized streaming for regular structured files
    OptimizedStream,
    /// Safe streaming for irregular or unknown files
    SafeStream,
    /// Progressive streaming with progress reporting for very large ranges
    Progressive,
}

/// Characteristics of a file determined by analysis
#[derive(Debug, Clone)]
pub struct FileCharacteristics {
    /// File size in bytes
    pub size_bytes: u64,
    /// Average line length
    pub avg_line_length: f64,
    /// Maximum line length found
    pub max_line_length: usize,
    /// Number of lines in file
    pub line_count: u64,
    /// Type of newlines detected
    pub newline_type: NewlineType,
    /// Whether file appears to be structured (regular line lengths)
    pub is_structured: bool,
}

/// Types of newline characters detected
#[derive(Debug, Clone, PartialEq)]
pub enum NewlineType {
    /// \n (Unix/Linux)
    LF,
    /// \r\n (Windows)
    Crlf,
    /// \r (Classic Mac)
    CR,
    /// Mixed or unknown
    Mixed,
}

/// Analyzes file samples to determine optimal reading strategy
pub struct FileAnalyzer;

impl FileAnalyzer {
    /// Analyzes a file sample (first 8KB) to determine characteristics
    pub async fn analyze_file_sample(path: &Path) -> Result<FileCharacteristics> {
        let mut file = File::open(path)
            .await
            .with_context(|| format!("Failed to open file {}", path.display()))?;

        // Get file size first
        let file_size = file.metadata().await?.len();

        // Read sample data (up to 8KB)
        let mut sample = vec![0; 8192];
        let bytes_read = file.read(&mut sample).await?;
        sample.truncate(bytes_read);

        // Analyze sample for characteristics
        Self::detect_file_characteristics(&sample, file_size)
    }

    /// Detects file characteristics from sample data
    pub fn detect_file_characteristics(
        sample: &[u8],
        total_size: u64,
    ) -> Result<FileCharacteristics> {
        if sample.is_empty() {
            return Ok(FileCharacteristics {
                size_bytes: total_size,
                avg_line_length: 0.0,
                max_line_length: 0,
                line_count: 0,
                newline_type: NewlineType::LF,
                is_structured: true,
            });
        }

        let sample_str = String::from_utf8_lossy(sample);
        let lines: Vec<&str> = sample_str.lines().collect();

        // Calculate line statistics
        let line_lengths: Vec<usize> = lines.iter().map(|line| line.len()).collect();
        let avg_line_length = line_lengths.iter().sum::<usize>() as f64 / line_lengths.len() as f64;
        let max_line_length = line_lengths.iter().max().copied().unwrap_or(0);

        // Detect newline type
        let newline_type = Self::detect_newline_type(&sample_str);

        // Determine if file is structured (low variance in line lengths)
        let variance = Self::calculate_variance(&line_lengths);
        let is_structured = variance < (avg_line_length * avg_line_length * 0.5); // variance threshold

        // Estimate total line count based on sample
        let estimated_lines = if total_size > 0 && !sample.is_empty() {
            (lines.len() as f64 * total_size as f64 / sample.len() as f64) as u64
        } else {
            lines.len() as u64
        };

        Ok(FileCharacteristics {
            size_bytes: total_size,
            avg_line_length,
            max_line_length,
            line_count: estimated_lines,
            newline_type,
            is_structured,
        })
    }

    /// Determines the optimal reading strategy based on file characteristics
    pub fn determine_optimal_strategy(characteristics: &FileCharacteristics) -> FileStrategy {
        // Small files: use full read
        if characteristics.size_bytes < 10 * 1024 * 1024 {
            return FileStrategy::FullRead;
        }

        // Very large files: use progressive streaming
        if characteristics.size_bytes > 1024 * 1024 * 1024 {
            return FileStrategy::Progressive;
        }

        // Structured files: use optimized streaming
        if characteristics.is_structured {
            return FileStrategy::OptimizedStream;
        }

        // Irregular files: use safe streaming
        FileStrategy::SafeStream
    }

    /// Detects the type of newlines in the text
    fn detect_newline_type(text: &str) -> NewlineType {
        let mut lf_count = 0;
        let mut cr_count = 0;
        let mut crlf_count = 0;

        let bytes = text.as_bytes();

        // Count different types of line endings
        for i in 0..bytes.len() {
            if bytes[i] == b'\n' {
                if i > 0 && bytes[i - 1] == b'\r' {
                    crlf_count += 1;
                } else {
                    lf_count += 1;
                }
            } else if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] != b'\n' {
                cr_count += 1;
            }
        }

        // Determine the type based on counts
        if crlf_count > 0 {
            if lf_count > 0 || cr_count > 0 {
                NewlineType::Mixed
            } else {
                NewlineType::Crlf
            }
        } else if lf_count > 0 && cr_count > 0 {
            NewlineType::Mixed
        } else if lf_count > 0 {
            NewlineType::LF
        } else if cr_count > 0 {
            NewlineType::CR
        } else {
            NewlineType::LF // Default to LF for empty text
        }
    }

    /// Calculates variance of line lengths
    fn calculate_variance(lengths: &[usize]) -> f64 {
        if lengths.is_empty() {
            return 0.0;
        }

        let mean = lengths.iter().sum::<usize>() as f64 / lengths.len() as f64;

        lengths
            .iter()
            .map(|&length| {
                let diff = length as f64 - mean;
                diff * diff
            })
            .sum::<f64>()
            / lengths.len() as f64
    }
}

/// Utility for efficiently skipping lines in large files
pub struct LineSkipper;

impl LineSkipper {
    /// Skips to the specified line number using chunk-based reading
    pub async fn skip_to_line_optimized(
        file: &mut File,
        target_line: u64,
        chunk_size: usize,
    ) -> Result<u64> {
        let mut current_line = 0;
        let mut position = 0u64;
        let mut buffer = vec![0; chunk_size];

        loop {
            if current_line >= target_line {
                break;
            }

            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break; // EOF
            }

            // Count newlines in this chunk
            let newlines_in_chunk = buffer[..bytes_read]
                .iter()
                .filter(|&&byte| byte == b'\n')
                .count() as u64;

            if current_line + newlines_in_chunk >= target_line {
                // Target line is within this chunk
                let remaining_lines = target_line - current_line;
                let mut line_counter = 0;

                for (i, &byte) in buffer[..bytes_read].iter().enumerate() {
                    if byte == b'\n' {
                        line_counter += 1;
                        if line_counter == remaining_lines {
                            position += i as u64 + 1;
                            break;
                        }
                    }
                }
                break;
            }

            current_line += newlines_in_chunk;
            position += bytes_read as u64;
        }

        Ok(position)
    }

    /// Adjusts file position to exact line boundary
    pub async fn adjust_to_exact_line_boundary(
        file: &mut File,
        start_position: u64,
    ) -> Result<u64> {
        file.seek(std::io::SeekFrom::Start(start_position)).await?;

        let mut buffer = vec![0; 1024];
        let mut position = start_position;
        let mut found_newline = false;

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }

            for (i, &byte) in buffer[..bytes_read].iter().enumerate() {
                if byte == b'\n' {
                    position += i as u64 + 1;
                    found_newline = true;
                    break;
                }
            }

            if found_newline {
                break;
            }

            position += bytes_read as u64;
        }

        Ok(position)
    }
}

/// Utility for controlled range reading with memory limits
pub struct RangeReader;

impl RangeReader {
    /// Reads content with size limits to prevent memory overflow
    pub async fn read_with_size_limit(
        file: &mut File,
        start_position: u64,
        max_lines: u64,
        max_bytes: usize,
    ) -> Result<(String, u64)> {
        file.seek(std::io::SeekFrom::Start(start_position)).await?;

        let mut result = String::new();
        let mut line_count = 0;
        let mut bytes_read = 0;
        let mut buffer = vec![0; 4096]; // 4KB buffer

        loop {
            if line_count >= max_lines || bytes_read >= max_bytes {
                break;
            }

            let chunk_bytes_read = file.read(&mut buffer).await?;
            if chunk_bytes_read == 0 {
                break; // EOF
            }

            let chunk_str = String::from_utf8_lossy(&buffer[..chunk_bytes_read]);

            // Count lines in this chunk
            let lines_in_chunk: Vec<&str> = chunk_str.lines().collect();

            for line in lines_in_chunk {
                if line_count >= max_lines {
                    break;
                }
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(line);
                line_count += 1;
            }

            bytes_read += chunk_bytes_read;
        }

        Ok((result, line_count))
    }

    /// Validates that the returned line count is correct
    pub fn validate_line_count(content: &str, expected_start: u64, expected_end: u64) -> bool {
        let actual_lines = content.lines().count() as u64;
        let expected_count = expected_end - expected_start + 1;
        actual_lines == expected_count
    }
}

#[cfg(test)]
mod tests {
    use tempfile::NamedTempFile;
    use tokio::fs;

    use super::*;

    #[tokio::test]
    async fn test_file_analyzer_characteristics() {
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n";
        let temp_file = NamedTempFile::new().unwrap();
        fs::write(temp_file.path(), content).await.unwrap();

        let characteristics = FileAnalyzer::analyze_file_sample(temp_file.path())
            .await
            .unwrap();

        assert_eq!(characteristics.size_bytes, content.len() as u64);
        assert!(characteristics.avg_line_length > 0.0);
        assert!(characteristics.max_line_length > 0);
        assert_eq!(characteristics.newline_type, NewlineType::LF);
        assert!(characteristics.is_structured);
    }

    #[tokio::test]
    async fn test_strategy_determination() {
        // Small file
        let small_chars = FileCharacteristics {
            size_bytes: 1024,
            avg_line_length: 50.0,
            max_line_length: 100,
            line_count: 20,
            newline_type: NewlineType::LF,
            is_structured: true,
        };
        assert_eq!(
            FileAnalyzer::determine_optimal_strategy(&small_chars),
            FileStrategy::FullRead
        );

        // Large structured file
        let large_structured_chars = FileCharacteristics {
            size_bytes: 100 * 1024 * 1024, // 100MB
            avg_line_length: 50.0,
            max_line_length: 100,
            line_count: 2000000,
            newline_type: NewlineType::LF,
            is_structured: true,
        };
        assert_eq!(
            FileAnalyzer::determine_optimal_strategy(&large_structured_chars),
            FileStrategy::OptimizedStream
        );

        // Very large file
        let very_large_chars = FileCharacteristics {
            size_bytes: 2 * 1024 * 1024 * 1024, // 2GB
            avg_line_length: 50.0,
            max_line_length: 100,
            line_count: 40000000,
            newline_type: NewlineType::LF,
            is_structured: true,
        };
        assert_eq!(
            FileAnalyzer::determine_optimal_strategy(&very_large_chars),
            FileStrategy::Progressive
        );
    }

    #[test]
    fn test_newline_detection() {
        assert_eq!(
            FileAnalyzer::detect_newline_type("line1\nline2\n"),
            NewlineType::LF
        );
        assert_eq!(
            FileAnalyzer::detect_newline_type("line1\r\nline2\r\n"),
            NewlineType::Crlf
        );
        assert_eq!(
            FileAnalyzer::detect_newline_type("line1\rline2\r"),
            NewlineType::CR
        );
        assert_eq!(
            FileAnalyzer::detect_newline_type("line1\nline2\r\n"),
            NewlineType::Mixed
        );
    }

    #[test]
    fn test_variance_calculation() {
        let uniform_lengths = [50, 50, 50, 50, 50];
        let uniform_variance = FileAnalyzer::calculate_variance(&uniform_lengths);
        assert_eq!(uniform_variance, 0.0);

        let varied_lengths = [10, 50, 100, 200, 5];
        let varied_variance = FileAnalyzer::calculate_variance(&varied_lengths);
        assert!(varied_variance > 0.0);
    }
}
