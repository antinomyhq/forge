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
    pub output: String,
    pub total_lines: u64,
    pub start_line: u64,
    pub end_line: u64,
}

const TRUNCATION_SUFFIX: &str = "...[Truncated]";

/// Truncates a line to a specified maximum length, appending a suffix if
/// truncated
fn truncate_line(line: String, max_length: usize) -> String {
    if line.chars().count() > max_length {
        let truncate_at = max_length.saturating_sub(TRUNCATION_SUFFIX.chars().count());
        format!(
            "{}{}",
            line.chars().take(truncate_at).collect::<String>(),
            TRUNCATION_SUFFIX
        )
    } else {
        line
    }
}

/// Truncates search output based on line limit, using search directory for
/// relative paths
pub fn truncate_search_output(
    output: &[Match],
    start_line: u64,
    max_lines: u64,
    max_line_length: usize,
    search_dir: &Path,
) -> TruncatedSearchOutput {
    let output = output
        .iter()
        .map(|v| format_match(v, search_dir))
        .map(|s| truncate_line(s, max_line_length))
        .collect::<Vec<_>>();

    // Count the actual number of lines in the output
    let total_lines = output.iter().map(|s| s.lines().count()).sum::<usize>() as u64;
    let is_truncated = total_lines > max_lines;

    let truncated_output = if is_truncated {
        output
            .iter()
            .skip(start_line as usize)
            .take(max_lines as usize)
            .map(String::from)
            .collect::<Vec<_>>()
    } else {
        output
    };

    TruncatedSearchOutput {
        output: truncated_output.join("\n"),
        total_lines,
        start_line: start_line + 1,
        end_line: if is_truncated {
            start_line + max_lines
        } else {
            total_lines
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_line() {
        // case 1: line exceeds the maximum length
        let long_line =
            "This is a very long line that exceeds the maximum length allowed for truncation."
                .to_string();
        let max_length = 50;
        let truncated = truncate_line(long_line, max_length);
        assert!(truncated.len() <= max_length);
        assert!(truncated.ends_with(TRUNCATION_SUFFIX));

        // case 2: line is within the maximum length
        let short_line = "This line is short enough.".to_string();
        let truncated_short = truncate_line(short_line.clone(), max_length);
        assert_eq!(truncated_short, short_line);
    }
}
