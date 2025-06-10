use std::path::PathBuf;

use crate::{FsCreateService, Services};

/// Number of lines to keep at the start of truncated output
pub(crate) const PREFIX_LINES: usize = 200;

/// Number of lines to keep at the end of truncated output
pub(crate) const SUFFIX_LINES: usize = 200;

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

/// Helper to process a stream and return (formatted_output, is_truncated)
fn process_stream(
    content: &str,
    tag: &str,
    prefix_lines: usize,
    suffix_lines: usize,
) -> (String, bool) {
    if content.trim().is_empty() {
        return (String::new(), false);
    }

    let (lines, truncation_info) = clip_by_lines(content, prefix_lines, suffix_lines);
    let is_truncated = truncation_info.is_some();
    let total_lines = content.lines().count();
    let output = tag_output(lines, truncation_info, tag, total_lines);

    (output, is_truncated)
}

/// Helper function to format potentially truncated output for stdout or stderr
fn tag_output(
    lines: Vec<String>,
    truncation_info: Option<(usize, usize)>,
    tag: &str,
    total_lines: usize,
) -> String {
    match truncation_info {
        Some((prefix_count, hidden_count)) => {
            let suffix_start_line = prefix_count + hidden_count + 1;
            let _suffix_count = lines.len() - prefix_count;

            let mut output = String::new();

            // Add prefix lines
            output.push_str(&format!("<{tag} lines=\"1-{prefix_count}\">\n"));
            for line in lines.iter().take(prefix_count) {
                output.push_str(line);
                output.push('\n');
            }
            output.push_str(&format!("</{tag}>\n"));

            // Add truncation marker
            output.push_str(&format!(
                "<truncated>...{tag} truncated ({hidden_count} lines not shown)...</truncated>\n"
            ));

            // Add suffix lines
            output.push_str(&format!(
                "<{tag} lines=\"{suffix_start_line}-{total_lines}\">\n"
            ));
            for line in lines.iter().skip(prefix_count) {
                output.push_str(line);
                output.push('\n');
            }
            output.push_str(&format!("</{tag}>\n"));

            output
        }
        None => {
            // No truncation, output all lines
            let mut output = format!("<{tag}>\n");
            for (i, line) in lines.iter().enumerate() {
                output.push_str(line);
                if i < lines.len() - 1 {
                    output.push('\n');
                }
            }
            output.push_str(&format!("\n</{tag}>"));
            output
        }
    }
}

/// Truncates shell output and creates a temporary file if needed
pub fn truncate_shell_output(stdout: &str, stderr: &str) -> TruncatedShellOutput {
    let (stdout_output, stdout_truncated) =
        process_stream(stdout, "stdout", PREFIX_LINES, SUFFIX_LINES);
    let (stderr_output, stderr_truncated) =
        process_stream(stderr, "stderr", PREFIX_LINES, SUFFIX_LINES);

    TruncatedShellOutput {
        stdout: stdout_output,
        stderr: stderr_output,
        stdout_truncated,
        stderr_truncated,
    }
}

/// Result of shell output truncation
pub struct TruncatedShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
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

/// Maximum search lines before truncation
pub const SEARCH_MAX_LINES: u64 = 200;

/// Represents the result of fs_search truncation
#[derive(Debug)]
pub struct TruncatedSearchOutput {
    pub output: String,
    pub path: String,
    pub regex: Option<String>,
    pub file_pattern: Option<String>,
    pub total_lines: u64,
    pub max_lines: u64,
}

/// Truncates search output based on line limit
pub fn truncate_search_output(
    output: &[String],
    path: &str,
    regex: Option<&String>,
    file_pattern: Option<&String>,
) -> TruncatedSearchOutput {
    let output = output.join("\n");
    let total_lines = output.lines().count() as u64;
    let is_truncated = total_lines > SEARCH_MAX_LINES;

    let truncated_output = if is_truncated {
        output
            .lines()
            .take(SEARCH_MAX_LINES as usize)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        output.to_string()
    };

    TruncatedSearchOutput {
        output: truncated_output,
        path: path.to_string(),
        regex: regex.map(|s| s.to_string()),
        file_pattern: file_pattern.map(|s| s.to_string()),
        total_lines,
        max_lines: SEARCH_MAX_LINES,
    }
}
