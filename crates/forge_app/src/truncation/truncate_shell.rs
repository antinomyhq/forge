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
#[derive(Debug, PartialEq)]
struct FormattedOutput {
    head: String,
    tail: Option<String>,
    suffix_start_line: Option<usize>,
    suffix_end_line: Option<usize>,
    prefix_end_line: usize,
}

/// Represents the result of processing a stream
#[derive(Debug, PartialEq)]
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

#[derive(Debug, PartialEq, derive_setters::Setters)]
#[setters(strip_option, into)]
pub struct Stdout {
    pub head: String,
    pub tail: Option<String>,
    pub total_lines: usize,
    pub head_end_line: usize,
    pub tail_start_line: Option<usize>,
    pub tail_end_line: Option<usize>,
}

#[derive(Debug, PartialEq, derive_setters::Setters)]
#[setters(strip_option, into)]
pub struct Stderr {
    pub head: String,
    pub tail: Option<String>,
    pub total_lines: usize,
    pub head_end_line: usize,
    pub tail_start_line: Option<usize>,
    pub tail_end_line: Option<usize>,
}

/// Result of shell output truncation
#[derive(Debug, PartialEq, derive_setters::Setters)]
#[setters(strip_option, into)]
pub struct TruncatedShellOutput {
    pub stdout: Stdout,
    pub stderr: Stderr,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    impl Stdout {
        pub fn new(head: String, total_lines: usize, head_end_line: usize) -> Self {
            Self {
                head,
                tail: None,
                total_lines,
                head_end_line,
                tail_start_line: None,
                tail_end_line: None,
            }
        }
    }

    impl Stderr {
        pub fn new(head: String, total_lines: usize, head_end_line: usize) -> Self {
            Self {
                head,
                tail: None,
                total_lines,
                head_end_line,
                tail_start_line: None,
                tail_end_line: None,
            }
        }
    }

    impl TruncatedShellOutput {
        pub fn new(stdout: Stdout, stderr: Stderr) -> Self {
            Self { stdout, stderr }
        }
    }

    #[test]
    fn test_no_truncation_needed() {
        let stdout = "line 1\nline 2\nline 3";
        let stderr = "error 1\nerror 2";

        let actual = truncate_shell_output(stdout, stderr, 5, 5);
        let expected = TruncatedShellOutput::new(
            Stdout::new("line 1\nline 2\nline 3".to_string(), 3, 3),
            Stderr::new("error 1\nerror 2".to_string(), 2, 2),
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_truncation_with_prefix_and_suffix() {
        let stdout = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7";
        let stderr = "error 1\nerror 2\nerror 3\nerror 4\nerror 5";

        let actual = truncate_shell_output(stdout, stderr, 2, 2);
        let expected = TruncatedShellOutput::new(
            Stdout::new("line 1\nline 2\n".to_string(), 7, 2)
                .tail("line 6\nline 7\n")
                .tail_start_line(6usize)
                .tail_end_line(7usize),
            Stderr::new("error 1\nerror 2\n".to_string(), 5, 2)
                .tail("error 4\nerror 5\n")
                .tail_start_line(4usize)
                .tail_end_line(5usize),
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_output() {
        let stdout = "";
        let stderr = "";

        let actual = truncate_shell_output(stdout, stderr, 5, 5);
        let expected = TruncatedShellOutput::new(
            Stdout::new("".to_string(), 0, 0),
            Stderr::new("".to_string(), 0, 0),
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_single_line_output() {
        let stdout = "single line";
        let stderr = "single error";

        let actual = truncate_shell_output(stdout, stderr, 2, 2);
        let expected = TruncatedShellOutput::new(
            Stdout::new("single line".to_string(), 1, 1),
            Stderr::new("single error".to_string(), 1, 1),
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_only_prefix_lines() {
        let stdout = "line 1\nline 2\nline 3\nline 4\nline 5";
        let stderr = "error 1\nerror 2\nerror 3";

        let actual = truncate_shell_output(stdout, stderr, 2, 0);
        let expected = TruncatedShellOutput::new(
            Stdout::new("line 1\nline 2\n".to_string(), 5, 2)
                .tail("")
                .tail_start_line(6usize)
                .tail_end_line(5usize),
            Stderr::new("error 1\nerror 2\n".to_string(), 3, 2)
                .tail("")
                .tail_start_line(4usize)
                .tail_end_line(3usize),
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_only_suffix_lines() {
        let stdout = "line 1\nline 2\nline 3\nline 4\nline 5";
        let stderr = "error 1\nerror 2\nerror 3";

        let actual = truncate_shell_output(stdout, stderr, 0, 2);
        let expected = TruncatedShellOutput::new(
            Stdout::new("".to_string(), 5, 0)
                .tail("line 4\nline 5\n")
                .tail_start_line(4usize)
                .tail_end_line(5usize),
            Stderr::new("".to_string(), 3, 0)
                .tail("error 2\nerror 3\n")
                .tail_start_line(2usize)
                .tail_end_line(3usize),
        );

        assert_eq!(actual, expected);
    }
}
