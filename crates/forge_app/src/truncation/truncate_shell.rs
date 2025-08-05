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
