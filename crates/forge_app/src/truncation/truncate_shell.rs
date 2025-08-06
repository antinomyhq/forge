/// Clips text content based on line count and optionally truncates long lines
fn clip_by_lines(
    content: &str,
    prefix_lines: usize,
    suffix_lines: usize,
    max_line_length: usize,
) -> (Vec<String>, Option<(usize, usize)>, usize) {
    let mut truncated_lines_count = 0;
    let lines = content
        .lines()
        .map(|line| {
            if line.len() > max_line_length {
                truncated_lines_count += 1;
                let extra_chars = line.len() - max_line_length;
                format!(
                    "{}...[{extra_chars} more chars truncated]",
                    &line[..max_line_length],
                )
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>();
    let total_lines = lines.len();

    // If content fits within limits, return all lines
    if total_lines <= prefix_lines.saturating_add(suffix_lines) {
        return (lines.into_iter().collect(), None, truncated_lines_count);
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
    (
        result_lines,
        Some((prefix_lines, hidden_lines)),
        truncated_lines_count,
    )
}

/// Represents formatted output with truncation metadata
#[derive(Debug, PartialEq)]
struct FormattedOutput {
    head: String,
    tail: Option<String>,
    suffix_start_line: Option<usize>,
    suffix_end_line: Option<usize>,
    prefix_end_line: usize,
    truncated_lines_count: usize,
}

/// Represents the result of processing a stream
#[derive(Debug, PartialEq)]
struct ProcessedStream {
    output: FormattedOutput,
    total_lines: usize,
}

/// Helper to process a stream and return structured output
fn process_stream(
    content: &str,
    prefix_lines: usize,
    suffix_lines: usize,
    max_line_length: usize,
) -> ProcessedStream {
    let (lines, truncation_info, truncated_lines_count) =
        clip_by_lines(content, prefix_lines, suffix_lines, max_line_length);
    let total_lines = content.lines().count();
    let output = tag_output(lines, truncation_info, total_lines, truncated_lines_count);

    ProcessedStream { output, total_lines }
}

/// Helper function to format potentially truncated output for stdout or stderr
fn tag_output(
    lines: Vec<String>,
    truncation_info: Option<(usize, usize)>,
    total_lines: usize,
    truncated_lines_count: usize,
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
                tail: if tail.is_empty() { None } else { Some(tail) },
                suffix_start_line: Some(suffix_start_line),
                suffix_end_line: Some(total_lines),
                prefix_end_line: prefix_count,
                truncated_lines_count,
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
                truncated_lines_count,
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
    max_line_length: usize,
) -> TruncatedShellOutput {
    let stdout_result = process_stream(stdout, prefix_lines, suffix_lines, max_line_length);
    let stderr_result = process_stream(stderr, prefix_lines, suffix_lines, max_line_length);

    TruncatedShellOutput::default()
        .stderr(Stderr {
            head: stderr_result.output.head,
            tail: stderr_result.output.tail,
            total_lines: stderr_result.total_lines,
            head_end_line: stderr_result.output.prefix_end_line,
            tail_start_line: stderr_result.output.suffix_start_line,
            tail_end_line: stderr_result.output.suffix_end_line,
            truncated_lines_count: stderr_result.output.truncated_lines_count,
        })
        .stdout(Stdout {
            head: stdout_result.output.head,
            tail: stdout_result.output.tail,
            total_lines: stdout_result.total_lines,
            head_end_line: stdout_result.output.prefix_end_line,
            tail_start_line: stdout_result.output.suffix_start_line,
            tail_end_line: stdout_result.output.suffix_end_line,
            truncated_lines_count: stdout_result.output.truncated_lines_count,
        })
}

#[derive(Debug, PartialEq, Default, derive_setters::Setters)]
#[setters(strip_option, into)]
pub struct Stdout {
    pub head: String,
    pub tail: Option<String>,
    pub total_lines: usize,
    pub head_end_line: usize,
    pub tail_start_line: Option<usize>,
    pub tail_end_line: Option<usize>,
    pub truncated_lines_count: usize,
}

#[derive(Debug, PartialEq, Default, derive_setters::Setters)]
#[setters(strip_option, into)]
pub struct Stderr {
    pub head: String,
    pub tail: Option<String>,
    pub total_lines: usize,
    pub head_end_line: usize,
    pub tail_start_line: Option<usize>,
    pub tail_end_line: Option<usize>,
    pub truncated_lines_count: usize,
}

/// Result of shell output truncation
#[derive(Debug, PartialEq, Default, derive_setters::Setters)]
#[setters(strip_option, into)]
pub struct TruncatedShellOutput {
    pub stdout: Stdout,
    pub stderr: Stderr,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_no_truncation_needed() {
        let stdout = "line 1\nline 2\nline 3";
        let stderr = "error 1\nerror 2";

        let actual = truncate_shell_output(stdout, stderr, 5, 5, 2000);
        let expected = TruncatedShellOutput::default()
            .stdout(
                Stdout::default()
                    .head("line 1\nline 2\nline 3")
                    .total_lines(3usize)
                    .head_end_line(3usize),
            )
            .stderr(
                Stderr::default()
                    .head("error 1\nerror 2")
                    .total_lines(2usize)
                    .head_end_line(2usize),
            );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_truncation_with_prefix_and_suffix() {
        let stdout = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7";
        let stderr = "error 1\nerror 2\nerror 3\nerror 4\nerror 5";

        let actual = truncate_shell_output(stdout, stderr, 2, 2, 2000);
        let expected = TruncatedShellOutput::default()
            .stdout(
                Stdout::default()
                    .head("line 1\nline 2\n")
                    .total_lines(7usize)
                    .head_end_line(2usize)
                    .tail("line 6\nline 7\n")
                    .tail_start_line(6usize)
                    .tail_end_line(7usize),
            )
            .stderr(
                Stderr::default()
                    .head("error 1\nerror 2\n")
                    .total_lines(5usize)
                    .head_end_line(2usize)
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

        let actual = truncate_shell_output(stdout, stderr, 5, 5, 2000);
        let expected = TruncatedShellOutput::default();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_single_line_output() {
        let stdout = "single line";
        let stderr = "single error";

        let actual = truncate_shell_output(stdout, stderr, 2, 2, 2000);
        let expected = TruncatedShellOutput::default()
            .stdout(
                Stdout::default()
                    .head("single line")
                    .total_lines(1usize)
                    .head_end_line(1usize),
            )
            .stderr(
                Stderr::default()
                    .head("single error")
                    .total_lines(1usize)
                    .head_end_line(1usize),
            );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_only_prefix_lines() {
        let stdout = "line 1\nline 2\nline 3\nline 4\nline 5";
        let stderr = "error 1\nerror 2\nerror 3";

        let actual = truncate_shell_output(stdout, stderr, 2, 0, 2000);
        let expected = TruncatedShellOutput::default()
            .stdout(
                Stdout::default()
                    .head("line 1\nline 2\n")
                    .total_lines(5usize)
                    .head_end_line(2usize)
                    .tail_start_line(6usize)
                    .tail_end_line(5usize),
            )
            .stderr(
                Stderr::default()
                    .head("error 1\nerror 2\n")
                    .total_lines(3usize)
                    .head_end_line(2usize)
                    .tail_start_line(4usize)
                    .tail_end_line(3usize),
            );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_only_suffix_lines() {
        let stdout = "line 1\nline 2\nline 3\nline 4\nline 5";
        let stderr = "error 1\nerror 2\nerror 3";

        let actual = truncate_shell_output(stdout, stderr, 0, 2, 2000);
        let expected = TruncatedShellOutput::default()
            .stdout(
                Stdout::default()
                    .total_lines(5usize)
                    .head_end_line(0usize)
                    .tail("line 4\nline 5\n")
                    .tail_start_line(4usize)
                    .tail_end_line(5usize),
            )
            .stderr(
                Stderr::default()
                    .total_lines(3usize)
                    .head_end_line(0usize)
                    .tail("error 2\nerror 3\n")
                    .tail_start_line(2usize)
                    .tail_end_line(3usize),
            );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_long_line() {
        let stdout = "line 1 \nline abcdefghijklmnopqrstuvwxyz\nline 2\nline 3\nline 4\nline 5";

        let actual = truncate_shell_output(stdout, "", usize::max_value(), usize::max_value(), 10);
        let expected = TruncatedShellOutput::default().stdout(
            Stdout::default()
                .head("line 1 \nline abcde...[21 more chars truncated]\nline 2\nline 3\nline 4\nline 5")
                .total_lines(6usize)
                .head_end_line(6usize)
                .truncated_lines_count(1usize),
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_line_truncation_with_multiple_long_lines() {
        let stdout = "short\nthis is a very long line that exceeds limit\nanother very long line that also exceeds the limit\nshort again";

        let actual = truncate_shell_output(stdout, "", usize::max_value(), usize::max_value(), 15);
        let expected = TruncatedShellOutput::default().stdout(
            Stdout::default()
                .head("short\nthis is a very ...[28 more chars truncated]\nanother very lo...[35 more chars truncated]\nshort again")
                .total_lines(4usize)
                .head_end_line(4usize)
                .truncated_lines_count(2usize),
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_line_truncation_with_line_count_truncation() {
        let stdout =
            "line 1\nvery long line that will be truncated\nline 3\nline 4\nline 5\nline 6\nline 7";

        let actual = truncate_shell_output(stdout, "", 2, 2, 10);
        let expected = TruncatedShellOutput::default().stdout(
            Stdout::default()
                .head("line 1\nvery long ...[27 more chars truncated]\n")
                .total_lines(7usize)
                .head_end_line(2usize)
                .tail("line 6\nline 7\n")
                .tail_start_line(6usize)
                .tail_end_line(7usize)
                .truncated_lines_count(1usize),
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_line_truncation_when_limit_not_set() {
        let stdout =
            "line 1\nvery long line that will not be truncated because no limit is set\nline 3";

        let actual =
            truncate_shell_output(stdout, "", usize::max_value(), usize::max_value(), 2000);
        let expected = TruncatedShellOutput::default().stdout(
            Stdout::default()
                .head("line 1\nvery long line that will not be truncated because no limit is set\nline 3")
                .total_lines(3usize)
                .head_end_line(3usize)
        );

        assert_eq!(actual, expected);
    }
}
