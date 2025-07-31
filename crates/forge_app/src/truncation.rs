use std::path::{Path, PathBuf};

use crate::utils::format_match;
use crate::{FsCreateService, Match};

pub async fn create_temp_file<S: FsCreateService>(
    services: &S,
    prefix: &str,
    ext: &str,
    content: &str,
) -> anyhow::Result<PathBuf> {
    let path = tempfile::Builder::new()
        .disable_cleanup(true)
        .prefix(prefix)
        .suffix(ext)
        .tempfile()?
        .into_temp_path()
        .to_path_buf();
    services
        .create(
            path.to_string_lossy().to_string(),
            content.to_string(),
            true,
            false,
        )
        .await?;
    Ok(path)
}

/// Clips text content based on line count
fn clip_by_lines(
    content: &str,
    prefix_lines: usize,
    suffix_lines: usize,
) -> (Vec<String>, Option<(usize, usize)>) {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // If content fits within limits, return all lines
    if total_lines <= prefix_lines + suffix_lines {
        return (lines.into_iter().map(String::from).collect(), None);
    }

    // Collect prefix and suffix lines
    let mut result_lines = Vec::new();

    // Add prefix lines
    for line in lines.iter().take(prefix_lines) {
        result_lines.push(line.to_string());
    }

    // Add suffix lines
    for line in lines.iter().skip(total_lines - suffix_lines) {
        result_lines.push(line.to_string());
    }

    // Return lines and truncation info (number of lines hidden)
    let hidden_lines = total_lines - prefix_lines - suffix_lines;
    (result_lines, Some((prefix_lines, hidden_lines)))
}

/// Represents formatted output with truncation metadata
#[derive(Debug)]
struct FormattedOutput {
    head: String,
    tail: Option<String>,
    suffix_start_line: Option<usize>,
    suffix_end_line: Option<usize>,
    prefix_end_line: usize,
}

/// Represents the result of processing a stream
#[derive(Debug)]
struct ProcessedStream {
    output: FormattedOutput,
    total_lines: usize,
}

/// Helper to process a stream and return structured output
fn process_stream(content: &str, prefix_lines: usize, suffix_lines: usize) -> ProcessedStream {
    let (lines, truncation_info) = clip_by_lines(content, prefix_lines, suffix_lines);
    let total_lines = content.lines().count();
    let output = tag_output(lines, truncation_info, total_lines);

    ProcessedStream { output, total_lines }
}

/// Helper function to format potentially truncated output for stdout or stderr
fn tag_output(
    lines: Vec<String>,
    truncation_info: Option<(usize, usize)>,
    total_lines: usize,
) -> FormattedOutput {
    match truncation_info {
        Some((prefix_count, hidden_count)) => {
            let suffix_start_line = prefix_count + hidden_count + 1;
            let mut head = String::new();
            let mut tail = String::new();

            // Add prefix lines
            for line in lines.iter().take(prefix_count) {
                head.push_str(line);
                head.push('\n');
            }

            // Add suffix lines
            for line in lines.iter().skip(prefix_count) {
                tail.push_str(line);
                tail.push('\n');
            }

            FormattedOutput {
                head,
                tail: Some(tail),
                suffix_start_line: Some(suffix_start_line),
                suffix_end_line: Some(total_lines),
                prefix_end_line: prefix_count,
            }
        }
        None => {
            // No truncation, output all lines
            let mut content = String::new();
            for (i, line) in lines.iter().enumerate() {
                content.push_str(line);
                if i < lines.len() - 1 {
                    content.push('\n');
                }
            }
            FormattedOutput {
                head: content,
                tail: None,
                suffix_start_line: None,
                suffix_end_line: None,
                prefix_end_line: total_lines,
            }
        }
    }
}

/// Truncates shell output and creates a temporary file if needed
pub fn truncate_shell_output(
    stdout: &str,
    stderr: &str,
    prefix_lines: usize,
    suffix_lines: usize,
) -> TruncatedShellOutput {
    let stdout_result = process_stream(stdout, prefix_lines, suffix_lines);
    let stderr_result = process_stream(stderr, prefix_lines, suffix_lines);

    TruncatedShellOutput {
        stderr: Stderr {
            head: stderr_result.output.head,
            tail: stderr_result.output.tail,
            total_lines: stderr_result.total_lines,
            head_end_line: stderr_result.output.prefix_end_line,
            tail_start_line: stderr_result.output.suffix_start_line,
            tail_end_line: stderr_result.output.suffix_end_line,
        },
        stdout: Stdout {
            head: stdout_result.output.head,
            tail: stdout_result.output.tail,
            total_lines: stdout_result.total_lines,
            head_end_line: stdout_result.output.prefix_end_line,
            tail_start_line: stdout_result.output.suffix_start_line,
            tail_end_line: stdout_result.output.suffix_end_line,
        },
    }
}

/// Trait for stream elements that can be converted to XML elements
pub trait StreamElement {
    fn stream_name(&self) -> &'static str;
    fn head_content(&self) -> &str;
    fn tail_content(&self) -> Option<&str>;
    fn total_lines(&self) -> usize;
    fn head_end_line(&self) -> usize;
    fn tail_start_line(&self) -> Option<usize>;
    fn tail_end_line(&self) -> Option<usize>;
}

impl StreamElement for Stdout {
    fn stream_name(&self) -> &'static str {
        "stdout"
    }

    fn head_content(&self) -> &str {
        &self.head
    }

    fn tail_content(&self) -> Option<&str> {
        self.tail.as_deref()
    }

    fn total_lines(&self) -> usize {
        self.total_lines
    }

    fn head_end_line(&self) -> usize {
        self.head_end_line
    }

    fn tail_start_line(&self) -> Option<usize> {
        self.tail_start_line
    }

    fn tail_end_line(&self) -> Option<usize> {
        self.tail_end_line
    }
}

impl StreamElement for Stderr {
    fn stream_name(&self) -> &'static str {
        "stderr"
    }

    fn head_content(&self) -> &str {
        &self.head
    }

    fn tail_content(&self) -> Option<&str> {
        self.tail.as_deref()
    }

    fn total_lines(&self) -> usize {
        self.total_lines
    }

    fn head_end_line(&self) -> usize {
        self.head_end_line
    }

    fn tail_start_line(&self) -> Option<usize> {
        self.tail_start_line
    }

    fn tail_end_line(&self) -> Option<usize> {
        self.tail_end_line
    }
}
pub struct Stdout {
    pub head: String,
    pub tail: Option<String>,
    pub total_lines: usize,
    pub head_end_line: usize,
    pub tail_start_line: Option<usize>,
    pub tail_end_line: Option<usize>,
}

pub struct Stderr {
    pub head: String,
    pub tail: Option<String>,
    pub total_lines: usize,
    pub head_end_line: usize,
    pub tail_start_line: Option<usize>,
    pub tail_end_line: Option<usize>,
}

/// Result of shell output truncation
pub struct TruncatedShellOutput {
    pub stdout: Stdout,
    pub stderr: Stderr,
}

/// Represents the result of fetch content truncation
#[derive(Debug)]
pub struct TruncatedFetchOutput {
    pub content: String,
}

/// Truncates fetch content based on character limit
pub fn truncate_fetch_content(content: &str, truncation_limit: usize) -> TruncatedFetchOutput {
    let original_length = content.len();
    let is_truncated = original_length > truncation_limit;

    let truncated_content = if is_truncated {
        content.chars().take(truncation_limit).collect()
    } else {
        content.to_string()
    };

    TruncatedFetchOutput { content: truncated_content }
}

/// Represents the result of fs_search truncation
#[derive(Debug)]
pub struct TruncatedSearchOutput {
    pub output: Truncator<String>,
    pub total_lines: usize,
    pub start_line: usize,
    pub end_line: usize,
}

/// Represents a truncation result for any collection type T.
/// Can hold collections that have been truncated by line count, byte size, or
/// not truncated at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Truncator<T> {
    Line(T),
    ByteSize(T),
    Full(T),
}

impl<T> From<T> for Truncator<T> {
    fn from(value: T) -> Self {
        Truncator::Full(value)
    }
}

impl<T, I> Truncator<T>
where
    T: IntoIterator<Item = I> + FromIterator<I> + Clone,
    T: AsRef<[I]>,
    I: AsRef<str>,
{
    /// Helper method to extract variant constructor and data without cloning
    fn into_parts(self) -> (fn(T) -> Self, T) {
        match self {
            Truncator::Line(data) => (Truncator::Line, data),
            Truncator::ByteSize(data) => (Truncator::ByteSize, data),
            Truncator::Full(data) => (Truncator::Full, data),
        }
    }

    /// Apply line truncation to any collection type T that can be iterated and
    /// collected. Works with Vec<String>, Vec<&str>, or any other collection
    /// implementing the required traits.
    pub fn truncate_items(self, start: usize, max_items: usize) -> Self {
        let (to_variant, input) = self.into_parts();

        let total_items = input.as_ref().len();
        let is_truncated = total_items > max_items;

        let truncated = if is_truncated {
            input.into_iter().skip(start).take(max_items).collect()
        } else {
            input
        };

        let truncated_len = truncated.as_ref().len();
        if truncated_len != total_items {
            Truncator::Line(truncated)
        } else {
            // preserve the original variant
            to_variant(truncated)
        }
    }

    /// Apply byte size truncation to any collection type T that can be
    /// iterated. Ensures the total byte size does not exceed the specified
    /// limit. Works with Vec<String>, Vec<&str>, or any other collection
    /// implementing the required traits.
    pub fn truncate_bytes(self, max_bytes: usize) -> Self {
        let (to_variant, input) = self.into_parts();

        let original_size = input.as_ref().len();

        let mut total_bytes = 0;
        let mut truncated = Vec::new();
        for item in input.into_iter() {
            let current_bytes = item.as_ref().len();
            total_bytes += current_bytes;
            if total_bytes >= max_bytes {
                break;
            }
            truncated.push(item);
        }

        let truncated_collection: T = truncated.into_iter().collect();
        let truncated_size = truncated_collection.as_ref().len();
        if original_size != truncated_size {
            Truncator::ByteSize(truncated_collection)
        } else {
            // preserve the original variant
            to_variant(truncated_collection)
        }
    }
}

impl<T> Truncator<T> {
    /// Maps the contained value of type T to type U using the provided
    /// function. Preserves the truncation variant (Line, ByteSize, or
    /// Full).
    pub fn map<F, U>(&self, f: F) -> Truncator<U>
    where
        F: FnOnce(&T) -> U,
        U: Clone,
    {
        match self {
            Truncator::Line(value) => Truncator::Line(f(value)),
            Truncator::ByteSize(value) => Truncator::ByteSize(f(value)),
            Truncator::Full(value) => Truncator::Full(f(value)),
        }
    }

    /// Returns true if the content was truncated (Line or ByteSize variants),
    /// false if the content is complete (Full variant).
    pub fn is_truncated(&self) -> bool {
        match self {
            Truncator::Line(_) => true,
            Truncator::ByteSize(_) => true,
            Truncator::Full(_) => false,
        }
    }

    /// Extracts the contained value of type T from the TruncationResult,
    /// consuming the TruncationResult in the process.
    pub fn unwrap(self) -> T {
        match self {
            Truncator::Line(value) => value,
            Truncator::ByteSize(value) => value,
            Truncator::Full(value) => value,
        }
    }
}

/// Truncates search output based on line limit, using search directory for
/// relative paths
pub fn truncate_search_output(
    output: &[Match],
    start_line: usize,
    max_lines: usize,
    max_bytes: usize,
    search_dir: &Path,
) -> TruncatedSearchOutput {
    let output = output
        .iter()
        .map(|v| format_match(v, search_dir))
        .collect::<Vec<_>>();
    let total_lines = output.len();

    // Apply truncation strategies
    let output = Truncator::from(output)
        .truncate_items(start_line, max_lines)
        .truncate_bytes(max_bytes)
        .map(|lines| lines.join("\n"));
    let total_lines_output = output.map(|out| out.lines().count()).unwrap();
    let start_line = if total_lines_output > start_line {
        start_line.saturating_add(1)
    } else {
        0
    };

    TruncatedSearchOutput {
        end_line: if output.is_truncated() {
            start_line
                .saturating_add(total_lines_output)
                .saturating_sub(1)
        } else {
            total_lines_output
        },
        output,
        total_lines,
        start_line,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_line_based_truncation() {
        let data = vec![
            "line 1".to_string(),
            "line 2".to_string(),
            "line 3".to_string(),
            "line 4".to_string(),
            "line 5".to_string(),
        ];

        let actual = Truncator::from(data.clone()).truncate_items(1, 3);
        let expected = Truncator::Line(data.into_iter().skip(1).take(3).collect::<Vec<_>>());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bytes_based_truncation() {
        // total entries = 5
        // each entry 5 bytes long
        // total size = 25 bytes
        let data = vec![
            "A".repeat(5),
            "B".repeat(5),
            "C".repeat(5),
            "D".repeat(5),
            "E".repeat(5),
        ];
        let actual = Truncator::from(data.clone()).truncate_bytes(20);
        let expected = Truncator::ByteSize(data.into_iter().take(3).collect::<Vec<_>>());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_both_truncation_strategies() {
        let data = vec![
            "A".repeat(900),
            "B".repeat(10),
            "C".repeat(25),
            "D".repeat(35),
            "E".repeat(45),
        ];
        let actual = Truncator::from(data.clone())
            .truncate_items(0, 10)
            .truncate_bytes(925);

        let expected = Truncator::ByteSize(data.into_iter().take(2).collect::<Vec<_>>());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_both_truncation_strategies_with_lower_byte_limit() {
        let data = vec![
            "A".repeat(900),
            "B".repeat(10),
            "C".repeat(25),
            "D".repeat(35),
            "E".repeat(45),
        ];
        let actual = Truncator::from(data.clone())
            .truncate_items(0, 10)
            .truncate_bytes(120);
        let expected = Truncator::ByteSize(vec![]);
        assert_eq!(actual, expected);
    }
}
