use std::path::PathBuf;

use crate::{FsCreateService, Services};

/// Number of lines to keep at the start of truncated output
const PREFIX_LINES: usize = 200;

/// Number of lines to keep at the end of truncated output
const SUFFIX_LINES: usize = 200;

/// Maximum characters for fetch content
const FETCH_MAX_LENGTH: usize = 40_000;

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
pub fn truncate_shell_output(stdout: &str, stderr: &str, command: &str) -> TruncatedShellOutput {
    let (stdout_output, stdout_truncated) =
        process_stream(stdout, "stdout", PREFIX_LINES, SUFFIX_LINES);
    let (stderr_output, stderr_truncated) =
        process_stream(stderr, "stderr", PREFIX_LINES, SUFFIX_LINES);

    TruncatedShellOutput {
        stdout: stdout_output,
        stderr: stderr_output,
        stdout_truncated,
        stderr_truncated,
        command: command.to_string(),
        original_stdout: stdout.to_string(),
        original_stderr: stderr.to_string(),
    }
}

/// Result of shell output truncation
pub struct TruncatedShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub command: String,
    pub original_stdout: String,
    pub original_stderr: String,
}

impl TruncatedShellOutput {
    /// Creates a temporary file if truncation occurred
    pub async fn create_temp_file_if_needed<S: Services>(
        &self,
        services: &S,
    ) -> anyhow::Result<Option<PathBuf>> {
        if self.stdout_truncated || self.stderr_truncated {
            let path = services
                .fs_create_service()
                .create_temp(
                    "forge_shell_",
                    ".md",
                    &format!(
                        "command:{}\n<stdout>{}</stdout>\n<stderr>{}</stderr>",
                        self.command, self.original_stdout, self.original_stderr
                    ),
                )
                .await?;

            Ok(Some(path))
        } else {
            Ok(None)
        }
    }
}

/// Represents the result of fetch content truncation
#[derive(Debug)]
pub struct TruncatedFetchOutput {
    pub content: String,
    pub url: String,
    pub code: u16,
    pub context: String,
    pub original_length: usize,
    pub start_char: usize,
    pub end_char: usize,
    pub max_length: usize,
    pub is_truncated: bool,
    pub original_content: String,
}

impl TruncatedFetchOutput {
    /// Creates a temp file if content was truncated
    pub async fn create_temp_file_if_needed<S: Services>(
        &self,
        services: &S,
    ) -> anyhow::Result<Option<PathBuf>> {
        if self.is_truncated {
            let path = services
                .fs_create_service()
                .create_temp("forge_fetch_", ".txt", &self.original_content)
                .await?;

            Ok(Some(path))
        } else {
            Ok(None)
        }
    }
}

/// Truncates fetch content based on character limit
pub fn truncate_fetch_content(
    content: &str,
    url: &str,
    code: u16,
    context: &str,
) -> TruncatedFetchOutput {
    let original_length = content.len();
    let end = FETCH_MAX_LENGTH.min(original_length);
    let is_truncated = original_length > FETCH_MAX_LENGTH;

    let truncated_content = if is_truncated {
        content.chars().take(FETCH_MAX_LENGTH).collect()
    } else {
        content.to_string()
    };

    TruncatedFetchOutput {
        content: truncated_content,
        url: url.to_string(),
        code,
        context: context.to_string(),
        original_length,
        start_char: 0,
        end_char: end,
        max_length: FETCH_MAX_LENGTH,
        is_truncated,
        original_content: content.to_string(),
    }
}
