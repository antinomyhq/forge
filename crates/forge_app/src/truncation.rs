use std::path::PathBuf;

use forge_domain::Environment;

use crate::utils::format_match;
use crate::{FsCreateService, Match, Services};

pub async fn create_temp_file<S: Services>(
    services: &S,
    prefix: &str,
    ext: &str,
    content: &str,
) -> anyhow::Result<PathBuf> {
    let path = tempfile::Builder::new()
        .keep(true)
        .prefix(prefix)
        .suffix(ext)
        .tempfile()?
        .into_temp_path()
        .to_path_buf();
    services
        .fs_create_service()
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
    content: String,
    prefix_count: usize,
    suffix_size: usize,
    hidden_count: usize,
}

/// Represents the result of processing a stream
#[derive(Debug)]
struct ProcessedStream {
    output: FormattedOutput,
    is_truncated: bool,
}

/// Helper to process a stream and return structured output
fn process_stream(content: &str, prefix_lines: usize, suffix_lines: usize) -> ProcessedStream {
    let (lines, truncation_info) = clip_by_lines(content, prefix_lines, suffix_lines);
    let is_truncated = truncation_info.is_some();
    let total_lines = content.lines().count();
    let output = tag_output(lines, truncation_info, total_lines);

    ProcessedStream { output, is_truncated }
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
            let suffix_size = total_lines - suffix_start_line + 1;

            let mut content = String::new();

            // Add prefix lines
            for line in lines.iter().take(prefix_count) {
                content.push_str(line);
                content.push('\n');
            }

            // Add truncation marker
            content.push_str(&format!("... [{hidden_count} lines omitted] ...\n"));

            // Add suffix lines
            for line in lines.iter().skip(prefix_count) {
                content.push_str(line);
                content.push('\n');
            }

            FormattedOutput { content, prefix_count, suffix_size, hidden_count }
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
                content,
                prefix_count: total_lines,
                suffix_size: total_lines,
                hidden_count: 0,
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
        stdout: stdout_result.output.content,
        stderr: stderr_result.output.content,
        stdout_truncated: stdout_result.is_truncated,
        stderr_truncated: stderr_result.is_truncated,
        stdout_prefix_count: stdout_result.output.prefix_count,
        stdout_suffix_size: stdout_result.output.suffix_size,
        stdout_hidden_count: stdout_result.output.hidden_count,
        stderr_prefix_count: stderr_result.output.prefix_count,
        stderr_hidden_count: stderr_result.output.hidden_count,
        stderr_suffix_size: stderr_result.output.suffix_size,
    }
}

/// Result of shell output truncation
pub struct TruncatedShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_prefix_count: usize,
    pub stdout_suffix_size: usize,
    pub stdout_hidden_count: usize,
    pub stderr_prefix_count: usize,
    pub stderr_hidden_count: usize,
    pub stderr_suffix_size: usize,
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

/// Truncates search output based on line limit
pub fn truncate_search_output(
    output: &[Match],
    start_line: u64,
    count: u64,
    env: &Environment,
) -> TruncatedSearchOutput {
    let total_outputs = output.len() as u64;
    let is_truncated = total_outputs > count;
    let output = output
        .iter()
        .map(|v| format_match(v, env))
        .collect::<Vec<_>>();

    let truncated_output = if is_truncated {
        output
            .iter()
            .skip(start_line as usize)
            .take(count as usize)
            .map(String::from)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        output.join("\n")
    };

    TruncatedSearchOutput {
        output: truncated_output,
        total_lines: total_outputs,
        start_line,
        end_line: if is_truncated {
            start_line + count
        } else {
            total_outputs
        },
    }
}
