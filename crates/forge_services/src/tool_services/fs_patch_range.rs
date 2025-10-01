use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use forge_app::domain::{LineRange, PatchRangeOperation};
use forge_app::{FsPatchRangeService, PatchOutput};
use thiserror::Error;
use tokio::fs;

use crate::utils::assert_absolute_path;
use crate::{FileWriterInfra, tool_services};

#[derive(Debug, Error)]
enum Error {
    #[error("Failed to read/write file: {0}")]
    FileOperation(#[from] std::io::Error),
    #[error("Invalid line range: start_line ({0}) must be at least 1")]
    InvalidStartLine(u32),
    #[error("Invalid line range: end_line ({0}) must be greater than or equal to start_line ({1})")]
    InvalidEndLine(u32, u32),
    #[error("Line range {0}-{1} is out of bounds (file has {2} lines)")]
    RangeOutOfBounds(u32, u32, usize),
    #[error("Overlapping ranges are not supported")]
    OverlappingRanges,
}

/// Validates that line ranges are valid and non-overlapping
fn validate_ranges(ranges: &[LineRange], total_lines: usize) -> Result<(), Error> {
    for range in ranges {
        // Validate start line
        if range.start_line == 0 {
            return Err(Error::InvalidStartLine(range.start_line));
        }

        // Validate end line if provided
        if let Some(end_line) = range.end_line {
            if end_line < range.start_line {
                return Err(Error::InvalidEndLine(end_line, range.start_line));
            }

            // Check bounds
            if end_line as usize > total_lines {
                return Err(Error::RangeOutOfBounds(
                    range.start_line,
                    end_line,
                    total_lines,
                ));
            }
        } else {
            // Single line - check bounds
            if range.start_line as usize > total_lines {
                return Err(Error::RangeOutOfBounds(
                    range.start_line,
                    range.start_line,
                    total_lines,
                ));
            }
        }
    }

    // Check for overlapping ranges
    let mut sorted_ranges: Vec<_> = ranges.iter().collect();
    sorted_ranges.sort_by_key(|r| r.start_line);

    for window in sorted_ranges.windows(2) {
        let first = window[0];
        let second = window[1];
        let first_end = first.end_line.unwrap_or(first.start_line);

        if second.start_line <= first_end {
            return Err(Error::OverlappingRanges);
        }
    }

    Ok(())
}

/// Applies a patch operation to specific line ranges in the content
fn apply_range_replacement(
    content: String,
    ranges: Vec<LineRange>,
    operation: &PatchRangeOperation,
    replacement_content: &str,
) -> Result<String, Error> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Validate ranges
    validate_ranges(&ranges, total_lines)?;

    // Sort ranges in reverse order to apply modifications from bottom to top
    // This ensures that earlier modifications don't affect line numbers of later
    // ranges
    let mut sorted_ranges = ranges;
    sorted_ranges.sort_by_key(|r| std::cmp::Reverse(r.start_line));

    let mut result_lines = lines.clone();

    for range in sorted_ranges {
        let start_idx = (range.start_line - 1) as usize; // Convert to 0-based
        let end_idx = range.end_line.unwrap_or(range.start_line) as usize;

        match operation {
            PatchRangeOperation::Prepend => {
                // Insert content before the start line
                result_lines.insert(start_idx, replacement_content);
            }
            PatchRangeOperation::Append => {
                // Insert content after the end line
                result_lines.insert(end_idx, replacement_content);
            }
            PatchRangeOperation::Replace => {
                // Remove the range and insert new content
                result_lines.drain(start_idx..end_idx);
                result_lines.insert(start_idx, replacement_content);
            }
        }
    }

    Ok(result_lines.join("\n"))
}

/// Modifies files by applying operations to specific line ranges. This provides
/// efficient line-based editing without requiring exact text matching.
pub struct ForgeFsPatchRange<F>(Arc<F>);

impl<F> ForgeFsPatchRange<F> {
    pub fn new(input: Arc<F>) -> Self {
        Self(input)
    }
}

#[async_trait::async_trait]
impl<F: FileWriterInfra> FsPatchRangeService for ForgeFsPatchRange<F> {
    async fn patch_range(
        &self,
        input_path: String,
        ranges: Vec<LineRange>,
        operation: PatchRangeOperation,
        content: String,
    ) -> anyhow::Result<PatchOutput> {
        let path = Path::new(&input_path);
        assert_absolute_path(path)?;

        // Read the original content
        let original_content = fs::read_to_string(path)
            .await
            .map_err(Error::FileOperation)?;

        // Apply the range-based replacement
        let modified_content =
            apply_range_replacement(original_content.clone(), ranges, &operation, &content)?;

        // Write the modified content to file
        self.0
            .write(path, Bytes::from(modified_content.clone()), true)
            .await?;

        Ok(PatchOutput {
            warning: tool_services::syn::validate(path, &modified_content).map(|e| e.to_string()),
            before: original_content,
            after: modified_content,
        })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_content() -> String {
        "line 1\nline 2\nline 3\nline 4\nline 5".to_string()
    }

    fn fixture_range(start: u32, end: Option<u32>) -> LineRange {
        LineRange { start_line: start, end_line: end }
    }

    #[test]
    fn test_apply_range_replacement_replace_single_line() {
        let source = fixture_content();
        let ranges = vec![fixture_range(2, None)];
        let operation = PatchRangeOperation::Replace;
        let content = "new line 2";

        let actual = apply_range_replacement(source, ranges, &operation, content).unwrap();

        let expected = "line 1\nnew line 2\nline 3\nline 4\nline 5";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_apply_range_replacement_replace_range() {
        let source = fixture_content();
        let ranges = vec![fixture_range(2, Some(4))];
        let operation = PatchRangeOperation::Replace;
        let content = "replacement";

        let actual = apply_range_replacement(source, ranges, &operation, content).unwrap();

        let expected = "line 1\nreplacement\nline 5";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_apply_range_replacement_prepend() {
        let source = fixture_content();
        let ranges = vec![fixture_range(3, None)];
        let operation = PatchRangeOperation::Prepend;
        let content = "prepended line";

        let actual = apply_range_replacement(source, ranges, &operation, content).unwrap();

        let expected = "line 1\nline 2\nprepended line\nline 3\nline 4\nline 5";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_apply_range_replacement_append() {
        let source = fixture_content();
        let ranges = vec![fixture_range(3, None)];
        let operation = PatchRangeOperation::Append;
        let content = "appended line";

        let actual = apply_range_replacement(source, ranges, &operation, content).unwrap();

        let expected = "line 1\nline 2\nline 3\nappended line\nline 4\nline 5";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_apply_range_replacement_multiple_ranges() {
        let source = fixture_content();
        let ranges = vec![fixture_range(1, None), fixture_range(5, None)];
        let operation = PatchRangeOperation::Replace;
        let content = "modified";

        let actual = apply_range_replacement(source, ranges, &operation, content).unwrap();

        let expected = "modified\nline 2\nline 3\nline 4\nmodified";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_validate_ranges_invalid_start_line() {
        let ranges = vec![fixture_range(0, None)];
        let total_lines = 5;

        let actual = validate_ranges(&ranges, total_lines);

        assert!(matches!(actual, Err(Error::InvalidStartLine(0))));
    }

    #[test]
    fn test_validate_ranges_invalid_end_line() {
        let ranges = vec![fixture_range(5, Some(3))];
        let total_lines = 5;

        let actual = validate_ranges(&ranges, total_lines);

        assert!(matches!(actual, Err(Error::InvalidEndLine(3, 5))));
    }

    #[test]
    fn test_validate_ranges_out_of_bounds() {
        let ranges = vec![fixture_range(1, Some(10))];
        let total_lines = 5;

        let actual = validate_ranges(&ranges, total_lines);

        assert!(matches!(actual, Err(Error::RangeOutOfBounds(1, 10, 5))));
    }

    #[test]
    fn test_validate_ranges_overlapping() {
        let ranges = vec![fixture_range(1, Some(3)), fixture_range(2, Some(4))];
        let total_lines = 5;

        let actual = validate_ranges(&ranges, total_lines);

        assert!(matches!(actual, Err(Error::OverlappingRanges)));
    }

    #[test]
    fn test_apply_range_replacement_append_to_range() {
        let source = fixture_content();
        let ranges = vec![fixture_range(2, Some(3))];
        let operation = PatchRangeOperation::Append;
        let content = "new content";

        let actual = apply_range_replacement(source, ranges, &operation, content).unwrap();

        let expected = "line 1\nline 2\nline 3\nnew content\nline 4\nline 5";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_apply_range_replacement_prepend_to_range() {
        let source = fixture_content();
        let ranges = vec![fixture_range(2, Some(3))];
        let operation = PatchRangeOperation::Prepend;
        let content = "new content";

        let actual = apply_range_replacement(source, ranges, &operation, content).unwrap();

        let expected = "line 1\nnew content\nline 2\nline 3\nline 4\nline 5";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_apply_range_replacement_multiline_content() {
        let source = fixture_content();
        let ranges = vec![fixture_range(3, None)];
        let operation = PatchRangeOperation::Replace;
        let content = "multi\nline\ncontent";

        let actual = apply_range_replacement(source, ranges, &operation, content).unwrap();

        let expected = "line 1\nline 2\nmulti\nline\ncontent\nline 4\nline 5";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_apply_range_replacement_empty_content() {
        let source = fixture_content();
        let ranges = vec![fixture_range(2, Some(4))];
        let operation = PatchRangeOperation::Replace;
        let content = "";

        let actual = apply_range_replacement(source, ranges, &operation, content).unwrap();

        let expected = "line 1\n\nline 5";
        assert_eq!(actual, expected);
    }
}
