// PathBuf now comes from the ShellInput in forge_domain
use std::sync::Arc;

use anyhow::bail;
use forge_app::EnvironmentService;
use forge_display::TitleFormat;
use forge_domain::{
    CommandOutput, Environment, ExecutableTool, NamedTool, Shell as ShellInput, ToolCallContext,
    ToolDescription, ToolName, ToolOutput,
};
use forge_tool_macros::ToolDescription;
use strip_ansi_escapes::strip;

use crate::metadata::Metadata;
use crate::{CommandExecutorService, FsWriteService, Infrastructure};

// Strips out the ansi codes from content.
fn strip_ansi(content: String) -> String {
    String::from_utf8_lossy(&strip(content.as_bytes())).into_owned()
}

/// Number of lines to keep at the start of truncated output
const PREFIX_LINES: usize = 200;

/// Number of lines to keep at the end of truncated output
const SUFFIX_LINES: usize = 200;

// Using ShellInput from forge_domain

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

/// Formats command output by wrapping non-empty stdout/stderr in XML tags.
/// stderr is commonly used for warnings and progress info, so success is
/// determined by exit status, not stderr presence. Returns Ok(output) on
/// success or Err(output) on failure, with a status message if both streams are
/// empty.
async fn format_output<F: Infrastructure>(
    infra: &Arc<F>,
    mut output: CommandOutput,
    keep_ansi: bool,
    prefix_lines: usize,
    suffix_lines: usize,
) -> anyhow::Result<String> {
    // Strip ANSI if needed
    if !keep_ansi {
        output.stdout = strip_ansi(output.stdout);
        output.stderr = strip_ansi(output.stderr);
    }

    // Build base metadata
    let mut metadata = Metadata::default()
        .add("command", &output.command)
        .add_optional("exit_code", output.exit_code);

    // Process streams
    let (stdout_output, stdout_truncated) =
        process_stream(&output.stdout, "stdout", prefix_lines, suffix_lines);
    let (stderr_output, stderr_truncated) =
        process_stream(&output.stderr, "stderr", prefix_lines, suffix_lines);

    // Update metadata for truncations
    if stdout_truncated {
        metadata = metadata.add("total_stdout_lines", output.stdout.lines().count());
    }
    if stderr_truncated {
        metadata = metadata.add("total_stderr_lines", output.stderr.lines().count());
    }

    // Combine outputs
    let mut outputs = vec![];
    if !stdout_output.is_empty() {
        outputs.push(stdout_output);
    }
    if !stderr_output.is_empty() {
        outputs.push(stderr_output);
    }

    let mut result = if outputs.is_empty() {
        format!(
            "Command {} with no output.",
            if output.success() {
                "executed successfully"
            } else {
                "failed"
            }
        )
    } else {
        outputs.join("\n")
    };

    // Handle truncation file if needed
    if stdout_truncated || stderr_truncated {
        let path = infra
            .file_write_service()
            .write_temp(
                "forge_shell_",
                ".md",
                &format!(
                    "command:{}\n<stdout>{}</stdout>\n<stderr>{}</stderr>",
                    output.command, output.stdout, output.stderr
                ),
            )
            .await?;

        metadata = metadata
            .add("temp_file", path.display())
            .add("truncated", "true");
        result.push_str(&format!(
            "\n<truncated>content is truncated, remaining content can be read from path:{}</truncated>",
            path.display()
        ));
    }

    // Return with appropriate error handling
    let final_output = format!("{metadata}{result}");
    if output.success() {
        Ok(final_output)
    } else {
        bail!(final_output)
    }
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

/// Executes shell commands with safety measures using restricted bash (rbash).
/// Prevents potentially harmful operations like absolute path execution and
/// directory changes. Use for file system interaction, running utilities,
/// installing packages, or executing build commands. For operations requiring
/// unrestricted access, advise users to run forge CLI with '-u' flag. Returns
/// complete output including stdout, stderr, and exit code for diagnostic
/// purposes.
#[derive(ToolDescription)]
pub struct Shell<I> {
    env: Environment,
    infra: Arc<I>,
}

impl<I: Infrastructure> Shell<I> {
    /// Create a new Shell with environment configuration
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.environment_service().get_environment();
        Self { env, infra }
    }

    fn validate_command(command: &str) -> anyhow::Result<()> {
        if command.trim().is_empty() {
            bail!("Command string is empty or contains only whitespace");
        }
        Ok(())
    }
}

impl<I> NamedTool for Shell<I> {
    fn tool_name() -> ToolName {
        ToolName::new("forge_tool_process_shell")
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> ExecutableTool for Shell<I> {
    type Input = ShellInput;

    async fn call(
        &self,
        context: &mut ToolCallContext,
        input: Self::Input,
    ) -> anyhow::Result<ToolOutput> {
        Self::validate_command(&input.command)?;

        let title_format = TitleFormat::debug(format!("Execute [{}]", self.env.shell.as_str()))
            .sub_title(&input.command);

        context.send_text(title_format).await?;

        let output = self
            .infra
            .command_executor_service()
            .execute_command(input.command, input.cwd)
            .await?;

        let result = format_output(
            &self.infra,
            output,
            input.keep_ansi,
            PREFIX_LINES,
            SUFFIX_LINES,
        )
        .await?;
        Ok(ToolOutput::text(result))
    }
}

#[cfg(test)]
mod tests {
    /// Test helper module to reduce boilerplate in tests
    mod helpers {
        use super::*;

        pub fn create_test_infra() -> Arc<MockInfrastructure> {
            Arc::new(MockInfrastructure::new())
        }

        pub fn create_command_output(
            stdout: &str,
            stderr: &str,
            command: &str,
            exit_code: Option<i32>,
        ) -> CommandOutput {
            CommandOutput {
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
                command: command.into(),
                exit_code,
            }
        }

        pub async fn format_output_test(
            stdout: &str,
            stderr: &str,
            command: &str,
            exit_code: Option<i32>,
            keep_ansi: bool,
            prefix_lines: usize,
            suffix_lines: usize,
        ) -> anyhow::Result<String> {
            let infra = create_test_infra();
            let output = create_command_output(stdout, stderr, command, exit_code);
            format_output(&infra, output, keep_ansi, prefix_lines, suffix_lines).await
        }
    }

    use helpers::*;
    #[tokio::test]
    async fn test_format_output_with_different_max_chars() {
        let infra = Arc::new(MockInfrastructure::new());

        // Test with small truncation values that will truncate the string
        let small_output = CommandOutput {
            stdout: "ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(),
            stderr: "".to_string(),
            command: "echo".into(),
            exit_code: Some(0),
        };
        let small_result = format_output(&infra, small_output, false, 5, 5)
            .await
            .unwrap();
        insta::assert_snapshot!(
            "format_output_small_truncation",
            TempDir::normalize(&small_result)
        );

        // Test with large values that won't cause truncation
        let large_output = CommandOutput {
            stdout: "ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(),
            stderr: "".to_string(),
            command: "echo".into(),
            exit_code: Some(0),
        };
        let large_result = format_output(&infra, large_output, false, 100, 100)
            .await
            .unwrap();
        insta::assert_snapshot!(
            "format_output_no_truncation",
            TempDir::normalize(&large_result)
        );
    }

    #[test]
    fn test_clip_by_lines_no_truncation() {
        let fixture = "line1\nline2\nline3";
        let (lines, truncation) = clip_by_lines(fixture, 5, 5);
        assert_eq!(lines, vec!["line1", "line2", "line3"]);
        assert_eq!(truncation, None);
    }

    #[test]
    fn test_clip_by_lines_with_truncation() {
        let fixture = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8";
        let (lines, truncation) = clip_by_lines(fixture, 2, 2);

        // Should have truncation info
        assert!(truncation.is_some());
        let (prefix_count, hidden_count) = truncation.unwrap();
        assert_eq!(prefix_count, 2);
        assert_eq!(hidden_count, 4); // 8 total - 2 prefix - 2 suffix = 4 hidden

        // Check the returned lines
        assert_eq!(lines.len(), 4); // 2 prefix + 2 suffix
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");
        assert_eq!(lines[2], "line7");
        assert_eq!(lines[3], "line8");
    }

    #[test]
    fn test_clip_by_lines_single_line() {
        let fixture = "single line";
        let (lines, truncation) = clip_by_lines(fixture, 1, 1);
        assert_eq!(lines, vec!["single line"]);
        assert_eq!(truncation, None);
    }

    #[test]
    fn test_clip_by_lines_empty_content() {
        let fixture = "";
        let (lines, truncation) = clip_by_lines(fixture, 5, 5);
        assert_eq!(lines.len(), 0);
        assert_eq!(truncation, None);
    }

    #[test]
    fn test_clip_by_lines_exact_boundary() {
        let fixture = "line1\nline2\nline3\nline4";
        let (lines, truncation) = clip_by_lines(fixture, 2, 2);
        // Exactly 4 lines with 2+2 limit should not truncate
        assert_eq!(truncation, None);
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_clip_by_lines_newline_handling() {
        let fixture = "line1\nline2\nline3\nline4\nline5\nline6";
        let (lines, truncation) = clip_by_lines(fixture, 2, 1);

        assert!(truncation.is_some());
        let (prefix_count, hidden_count) = truncation.unwrap();
        assert_eq!(prefix_count, 2);
        assert_eq!(hidden_count, 3); // 6 total - 2 prefix - 1 suffix = 3 hidden

        assert_eq!(lines.len(), 3); // 2 prefix + 1 suffix
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");
        assert_eq!(lines[2], "line6");
    }

    #[test]
    fn test_strip_ansi_with_codes() {
        let fixture = "\x1b[32mGreen text\x1b[0m\x1b[1mBold\x1b[0m".to_string();
        let actual = strip_ansi(fixture);
        let expected = "Green textBold";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strip_ansi_without_codes() {
        let fixture = "Plain text".to_string();
        let actual = strip_ansi(fixture.clone());
        let expected = fixture;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strip_ansi_empty() {
        let fixture = "".to_string();
        let actual = strip_ansi(fixture);
        let expected = "";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_tag_output_no_truncation() {
        let lines = vec!["simple output".to_string()];
        let actual = tag_output(lines, None, "stdout", 1);
        let expected = "<stdout>\nsimple output\n</stdout>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_tag_output_with_truncation() {
        let lines = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line8".to_string(),
            "line9".to_string(),
        ];
        let truncation_info = Some((2, 4)); // 2 prefix lines, 4 hidden lines
        let actual = tag_output(lines, truncation_info, "stderr", 9);

        // Check for expected content
        assert!(actual.contains("<stderr lines=\"1-2\">"));
        assert!(actual.contains("line1\nline2\n"));
        assert!(actual.contains("<stderr lines=\"7-9\">"));
        assert!(actual.contains("line8\nline9\n"));
        assert!(actual.contains("truncated (4 lines not shown)"));
    }

    #[tokio::test]
    async fn test_format_output_empty_stdout_stderr() {
        let actual = format_output_test("", "", "true", Some(0), false, 100, 100)
            .await
            .unwrap();
        assert!(actual.contains("Command executed successfully with no output"));
    }

    #[tokio::test]
    async fn test_format_output_empty_with_failure() {
        let result = format_output_test("", "", "false", Some(-1), false, 100, 100).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Command failed with no output"));
    }

    #[tokio::test]
    async fn test_format_output_whitespace_only() {
        let actual = format_output_test("   \t\n  ", "  \n\t  ", "echo", Some(0), false, 100, 100)
            .await
            .unwrap();
        // Whitespace-only output should be treated as empty
        assert!(actual.contains("Command executed successfully with no output"));
    }

    #[tokio::test]
    async fn test_format_output_metadata_fields() {
        let infra = Arc::new(MockInfrastructure::new());
        let fixture = CommandOutput {
            stdout: "test output".to_string(),
            stderr: "".to_string(),
            command: "test command".into(),
            exit_code: Some(42),
        };
        let actual = format_output(&infra, fixture, false, 100, 100)
            .await
            .unwrap();

        assert!(actual.contains("command: test command"));
        assert!(actual.contains("exit_code: 42"));
        assert!(actual.contains("<stdout>\ntest output"));
    }

    #[tokio::test]
    async fn test_format_output_no_exit_code() {
        let infra = Arc::new(MockInfrastructure::new());
        let fixture = CommandOutput {
            stdout: "output".to_string(),
            stderr: "".to_string(),
            command: "test".into(),
            exit_code: None,
        };
        let actual = format_output(&infra, fixture, false, 100, 100)
            .await
            .unwrap();

        assert!(actual.contains("command: test"));
        // Should not contain exit_code field when it's None
        assert!(!actual.contains("exit_code"));
    }

    #[tokio::test]
    async fn test_shell_whitespace_command() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "   \t  ".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Command string is empty or contains only whitespace"
        );
    }

    #[tokio::test]
    async fn test_shell_command_with_quotes() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "echo \"hello world\"".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: false,
                    explanation: None,
                },
            )
            .await
            .unwrap();
        // The MockInfrastructure will return "hello world\n" for echo commands
        // which gets wrapped in metadata and stdout tags
        assert!(result.contains("hello world"));
    }

    #[tokio::test]
    async fn test_shell_keep_ansi_false() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "echo test".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: false,
                    explanation: None,
                },
            )
            .await
            .unwrap();
        // The MockInfrastructure will return "test\n" for this echo command
        assert!(result.contains("test"));
    }

    #[tokio::test]
    async fn test_shell_with_explanation() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "echo test".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: Some("Testing echo command".to_string()),
                },
            )
            .await
            .unwrap();
        // The MockInfrastructure will return "test\n" for this echo command
        assert!(result.contains("test"));
    }

    #[test]
    fn test_tool_name() {
        let actual = Shell::<MockInfrastructure>::tool_name();
        let expected = ToolName::new("forge_tool_process_shell");
        assert_eq!(actual, expected);
    }
    use std::env;
    use std::sync::Arc;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;
    use crate::utils::{TempDir, ToolContentExtension};

    /// Platform-specific error message patterns for command not found errors
    #[cfg(target_os = "windows")]
    const COMMAND_NOT_FOUND_PATTERNS: [&str; 2] = [
        "is not recognized",             // cmd.exe
        "'non_existent_command' is not", // PowerShell
    ];

    #[cfg(target_family = "unix")]
    const COMMAND_NOT_FOUND_PATTERNS: [&str; 3] = [
        "command not found",               // bash/sh
        "non_existent_command: not found", // bash/sh (Alternative Unix error)
        "No such file or directory",       // Alternative Unix error
    ];

    #[tokio::test]
    async fn test_shell_echo() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "echo 'Hello, World!'".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await
            .unwrap();
        assert!(result.contains("Mock command executed successfully"));
    }

    #[tokio::test]
    async fn test_shell_stderr_with_success() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);
        // Use a command that writes to both stdout and stderr
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: if cfg!(target_os = "windows") {
                        "echo 'to stderr' 1>&2 && echo 'to stdout'".to_string()
                    } else {
                        "echo 'to stderr' >&2; echo 'to stdout'".to_string()
                    },
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await
            .unwrap();
        insta::assert_snapshot!(&result.into_string());
    }

    #[tokio::test]
    async fn test_shell_both_streams() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "echo 'to stdout' && echo 'to stderr' >&2".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await
            .unwrap();
        insta::assert_snapshot!(&result.into_string());
    }

    #[tokio::test]
    async fn test_shell_with_working_directory() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);
        let temp_dir = TempDir::new().unwrap().path();

        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: if cfg!(target_os = "windows") {
                        "cd".to_string()
                    } else {
                        "pwd".to_string()
                    },
                    cwd: temp_dir.clone(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await
            .unwrap();
        insta::assert_snapshot!(
            "format_output_working_directory",
            TempDir::normalize(&result.into_string())
        );
    }

    #[tokio::test]
    async fn test_shell_invalid_command() {
        let shell = Shell::new(Arc::new(MockInfrastructure::new()));
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "non_existent_command".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();

        // Check if any of the platform-specific patterns match
        let matches_pattern = COMMAND_NOT_FOUND_PATTERNS
            .iter()
            .any(|&pattern| err.to_string().contains(pattern));

        assert!(
            matches_pattern,
            "Error message '{err}' did not match any expected patterns for this platform: {COMMAND_NOT_FOUND_PATTERNS:?}"
        );
    }

    #[tokio::test]
    async fn test_shell_empty_command() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Command string is empty or contains only whitespace"
        );
    }

    #[tokio::test]
    async fn test_description() {
        assert!(
            Shell::new(Arc::new(MockInfrastructure::new()))
                .description()
                .len()
                > 100
        )
    }

    #[tokio::test]
    async fn test_shell_pwd() {
        let shell = Shell::new(Arc::new(MockInfrastructure::new()));

        // Use a temporary directory to make the test more predictable
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: if cfg!(target_os = "windows") {
                        "cd".to_string()
                    } else {
                        "pwd".to_string()
                    },
                    cwd: temp_path.clone(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await
            .unwrap();

        insta::assert_snapshot!("test_shell_pwd", TempDir::normalize(&result.into_string()));
    }

    #[tokio::test]
    async fn test_shell_multiple_commands() {
        let shell = Shell::new(Arc::new(MockInfrastructure::new()));
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "echo 'first' && echo 'second'".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await
            .unwrap();
        insta::assert_snapshot!(&result.into_string());
    }

    #[tokio::test]
    async fn test_shell_empty_output() {
        let shell = Shell::new(Arc::new(MockInfrastructure::new()));
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "true".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await
            .unwrap();

        assert!(result.contains("executed successfully"));
        assert!(!result.contains("failed"));
    }

    #[tokio::test]
    async fn test_shell_whitespace_only_output() {
        let shell = Shell::new(Arc::new(MockInfrastructure::new()));
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "echo ''".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await
            .unwrap();

        assert!(result.contains("executed successfully"));
        assert!(!result.contains("failed"));
    }

    #[tokio::test]
    async fn test_shell_with_environment_variables() {
        let shell = Shell::new(Arc::new(MockInfrastructure::new()));
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "echo $PATH".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await
            .unwrap();

        assert!(!result.contains("Error:"));
    }

    #[tokio::test]
    async fn test_shell_full_path_command() {
        let shell = Shell::new(Arc::new(MockInfrastructure::new()));
        // Using a full path command which would be restricted in rbash
        let cmd = if cfg!(target_os = "windows") {
            r"C:\Windows\System32\whoami.exe"
        } else {
            "/bin/ls"
        };

        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: cmd.to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await;

        // In rbash, this would fail with a permission error
        // For our normal shell test, it should succeed
        assert!(
            result.is_ok(),
            "Full path commands should work in normal shell"
        );
    }

    #[tokio::test]
    async fn test_format_output_ansi_handling() {
        let infra = Arc::new(MockInfrastructure::new());
        // Test with keep_ansi = true (should preserve ANSI codes)
        let ansi_output = CommandOutput {
            stdout: "\x1b[32mSuccess\x1b[0m".to_string(),
            stderr: "\x1b[31mWarning\x1b[0m".to_string(),
            command: "ls -la".into(),
            exit_code: Some(0),
        };
        let preserved = format_output(&infra, ansi_output, true, PREFIX_LINES, SUFFIX_LINES)
            .await
            .unwrap();
        insta::assert_snapshot!("format_output_ansi_preserved", preserved);

        // Test with keep_ansi = false (should strip ANSI codes)
        let ansi_output = CommandOutput {
            stdout: "\x1b[32mSuccess\x1b[0m".to_string(),
            stderr: "\x1b[31mWarning\x1b[0m".to_string(),
            command: "ls -la".into(),
            exit_code: Some(0),
        };
        let stripped = format_output(&infra, ansi_output, false, PREFIX_LINES, SUFFIX_LINES)
            .await
            .unwrap();
        insta::assert_snapshot!("format_output_ansi_stripped", stripped);
    }

    #[tokio::test]
    async fn test_format_output_with_large_command_output() {
        let infra = Arc::new(MockInfrastructure::new());
        // Using tiny PREFIX_CHARS and SUFFIX_CHARS values (30) to test truncation with
        // minimal content This creates very small snapshots while still testing
        // the truncation logic
        const TINY_PREFIX: usize = 30;
        const TINY_SUFFIX: usize = 30;

        // Create a test string just long enough to trigger truncation with our small
        // prefix/suffix values
        let test_string = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".repeat(4); // 104 characters

        let ansi_output = CommandOutput {
            stdout: test_string.clone(),
            stderr: test_string,
            command: "ls -la".into(),
            exit_code: Some(0),
        };

        let preserved = format_output(&infra, ansi_output, false, TINY_PREFIX, TINY_SUFFIX)
            .await
            .unwrap();
        // Use a specific name for the snapshot instead of auto-generated name
        insta::assert_snapshot!(
            "format_output_large_command",
            TempDir::normalize(&preserved)
        );
    }
    #[tokio::test]
    async fn test_format_output_multiline_stdout() {
        let infra = Arc::new(MockInfrastructure::new());
        let fixture = CommandOutput {
            stdout: "line1\nline2\nline3\nline4\nline5".to_string(),
            stderr: "".to_string(),
            command: "echo multiline".into(),
            exit_code: Some(0),
        };
        let actual = format_output(&infra, fixture, false, 100, 100)
            .await
            .unwrap();
        insta::assert_snapshot!("format_output_multiline_stdout", actual);
    }

    #[tokio::test]
    async fn test_format_output_multiline_stderr() {
        let infra = Arc::new(MockInfrastructure::new());
        let fixture = CommandOutput {
            stdout: "".to_string(),
            stderr: "error line 1\nerror line 2\nerror line 3".to_string(),
            command: "test command".into(),
            exit_code: Some(0),
        };
        let actual = format_output(&infra, fixture, false, 100, 100)
            .await
            .unwrap();
        insta::assert_snapshot!("format_output_multiline_stderr", actual);
    }

    #[tokio::test]
    async fn test_format_output_multiline_both_streams() {
        let infra = Arc::new(MockInfrastructure::new());
        let fixture = CommandOutput {
            stdout: "stdout line 1\nstdout line 2\nstdout line 3".to_string(),
            stderr: "stderr line 1\nstderr line 2".to_string(),
            command: "complex command".into(),
            exit_code: Some(0),
        };
        let actual = format_output(&infra, fixture, false, 100, 100)
            .await
            .unwrap();
        insta::assert_snapshot!("format_output_multiline_both_streams", actual);
    }

    #[tokio::test]
    async fn test_format_output_multiline_with_line_truncation() {
        let infra = Arc::new(MockInfrastructure::new());
        // Create content with many lines to test line-based truncation
        let many_lines = (1..=20)
            .map(|i| format!("This is line number {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let fixture = CommandOutput {
            stdout: many_lines.clone(),
            stderr: many_lines,
            command: "generate many lines".into(),
            exit_code: Some(0),
        };

        // Use small line limits to force truncation
        let actual = format_output(&infra, fixture, false, 3, 3).await.unwrap();
        insta::assert_snapshot!(
            "format_output_multiline_line_truncation",
            TempDir::normalize(&actual)
        );
    }

    #[tokio::test]
    async fn test_format_output_stdout_only_truncation() {
        let infra = Arc::new(MockInfrastructure::new());
        // Create content where only stdout gets truncated
        let many_stdout_lines = (1..=15)
            .map(|i| format!("stdout line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let few_stderr_lines = "error line 1\nerror line 2".to_string();

        let fixture = CommandOutput {
            stdout: many_stdout_lines,
            stderr: few_stderr_lines,
            command: "stdout truncation test".into(),
            exit_code: Some(0),
        };

        // Use limits that will truncate stdout but not stderr
        let actual = format_output(&infra, fixture, false, 3, 3).await.unwrap();
        insta::assert_snapshot!(
            "format_output_stdout_only_truncation",
            TempDir::normalize(&actual)
        );
    }

    #[tokio::test]
    async fn test_format_output_stderr_only_truncation() {
        let infra = Arc::new(MockInfrastructure::new());
        // Create content where only stderr gets truncated
        let few_stdout_lines = "output line 1\noutput line 2".to_string();
        let many_stderr_lines = (1..=15)
            .map(|i| format!("stderr line {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let fixture = CommandOutput {
            stdout: few_stdout_lines,
            stderr: many_stderr_lines,
            command: "stderr truncation test".into(),
            exit_code: Some(1),
        };

        // Use limits that will truncate stderr but not stdout
        let actual = format_output(&infra, fixture, false, 3, 3).await.unwrap();
        insta::assert_snapshot!(
            "format_output_stderr_only_truncation",
            TempDir::normalize(&actual)
        );
    }

    #[tokio::test]
    async fn test_format_output_single_line_truncation() {
        let infra = Arc::new(MockInfrastructure::new());
        // Test truncation with very minimal limits
        let single_long_output = (1..=10)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let fixture = CommandOutput {
            stdout: single_long_output,
            stderr: "".to_string(),
            command: "single line truncation test".into(),
            exit_code: Some(0),
        };

        // Use very small limits to test edge case
        let actual = format_output(&infra, fixture, false, 1, 1).await.unwrap();
        insta::assert_snapshot!(
            "format_output_single_line_truncation",
            TempDir::normalize(&actual)
        );
    }

    #[tokio::test]
    async fn test_format_output_asymmetric_truncation() {
        let infra = Arc::new(MockInfrastructure::new());
        // Test with different prefix/suffix ratios
        let many_lines = (1..=20)
            .map(|i| format!("asymmetric line {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let fixture = CommandOutput {
            stdout: many_lines,
            stderr: "".to_string(),
            command: "asymmetric truncation test".into(),
            exit_code: Some(0),
        };

        // Use asymmetric limits (more prefix than suffix)
        let actual = format_output(&infra, fixture, false, 5, 2).await.unwrap();
        insta::assert_snapshot!(
            "format_output_asymmetric_truncation",
            TempDir::normalize(&actual)
        );
    }

    #[tokio::test]
    async fn test_format_output_boundary_truncation() {
        let infra = Arc::new(MockInfrastructure::new());
        // Test exactly at the boundary where truncation would occur
        let exact_boundary_lines = (1..=10)
            .map(|i| format!("boundary line {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let fixture = CommandOutput {
            stdout: exact_boundary_lines,
            stderr: "".to_string(),
            command: "boundary truncation test".into(),
            exit_code: Some(0),
        };

        // Use limits that exactly match the content (should not truncate)
        let actual = format_output(&infra, fixture, false, 5, 5).await.unwrap();
        insta::assert_snapshot!(
            "format_output_boundary_no_truncation",
            TempDir::normalize(&actual)
        );
    }

    #[tokio::test]
    async fn test_format_output_mixed_content_truncation() {
        let infra = Arc::new(MockInfrastructure::new());
        // Test with mixed content including empty lines and special characters
        let mixed_stdout = [
            "normal line 1",
            "",
            "line with special chars: !@#$%^&*()",
            "line with unicode: 🚀 🎉 ✨",
            "",
            "another normal line",
            "line with tabs:\tindented",
            "final line",
        ]
        .join("\n");

        let mixed_stderr = [
            "error: something went wrong",
            "",
            "stack trace line 1",
            "stack trace line 2",
            "stack trace line 3",
            "",
            "final error message",
        ]
        .join("\n");

        let fixture = CommandOutput {
            stdout: mixed_stdout,
            stderr: mixed_stderr,
            command: "mixed content test".into(),
            exit_code: Some(1),
        };

        // Use small limits to force truncation of mixed content
        let actual = format_output(&infra, fixture, false, 2, 2).await.unwrap();
        insta::assert_snapshot!(
            "format_output_mixed_content_truncation",
            TempDir::normalize(&actual)
        );
    }

    #[tokio::test]
    async fn test_format_output_large_content_with_temp_file() {
        let infra = Arc::new(MockInfrastructure::new());
        // Create very large content that will trigger temp file creation
        let large_stdout = (1..=500)
            .map(|i| format!("This is a very long stdout line number {i} with lots of content to make it exceed normal limits"))
            .collect::<Vec<_>>()
            .join("\n");

        let large_stderr = (1..=300)
            .map(|i| {
                format!(
                    "This is a very long stderr line number {i} with error details and stack traces"
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let fixture = CommandOutput {
            stdout: large_stdout,
            stderr: large_stderr,
            command: "large content generation".into(),
            exit_code: Some(0),
        };

        // Use small limits to force both truncation and temp file creation
        let actual = format_output(&infra, fixture, false, 5, 5).await.unwrap();
        insta::assert_snapshot!(
            "format_output_large_content_with_temp_file",
            TempDir::normalize(&actual)
        );
    }

    #[tokio::test]
    async fn test_format_output_multiline_mixed_newlines() {
        let infra = Arc::new(MockInfrastructure::new());
        let fixture = CommandOutput {
            stdout: "line1\n\nline3\n\n\nline6".to_string(), // Mixed empty lines
            stderr: "error1\nerror2\n".to_string(),          // Trailing newline
            command: "mixed newlines".into(),
            exit_code: Some(0),
        };
        let actual = format_output(&infra, fixture, false, 100, 100)
            .await
            .unwrap();
        insta::assert_snapshot!("format_output_multiline_mixed_newlines", actual);
    }

    #[tokio::test]
    async fn test_format_output_multiline_very_long_lines() {
        let infra = Arc::new(MockInfrastructure::new());
        // Create a few very long lines
        let long_line1 = "A".repeat(200);
        let long_line2 = "B".repeat(150);
        let long_line3 = "C".repeat(100);

        let fixture = CommandOutput {
            stdout: format!("{long_line1}\n{long_line2}\n{long_line3}"),
            stderr: "".to_string(),
            command: "long lines".into(),
            exit_code: Some(0),
        };

        // Test with line-based truncation on long lines
        let actual = format_output(&infra, fixture, false, 2, 1).await.unwrap();
        insta::assert_snapshot!(
            "format_output_multiline_long_lines",
            TempDir::normalize(&actual)
        );
    }

    #[tokio::test]
    async fn test_format_output_multiline_with_ansi_codes() {
        let infra = Arc::new(MockInfrastructure::new());
        let fixture = CommandOutput {
            stdout:
                "\x1b[32mGreen line 1\x1b[0m\n\x1b[31mRed line 2\x1b[0m\n\x1b[1mBold line 3\x1b[0m"
                    .to_string(),
            stderr: "\x1b[33mYellow error 1\x1b[0m\n\x1b[35mMagenta error 2\x1b[0m".to_string(),
            command: "colorized output".into(),
            exit_code: Some(0),
        };

        // Test with ANSI codes preserved
        let actual_preserved = format_output(&infra, CommandOutput {
            stdout: "\x1b[32mGreen line 1\x1b[0m\n\x1b[31mRed line 2\x1b[0m\n\x1b[1mBold line 3\x1b[0m".to_string(),
            stderr: "\x1b[33mYellow error 1\x1b[0m\n\x1b[35mMagenta error 2\x1b[0m".to_string(),
            command: "colorized output".into(),
            exit_code: Some(0),
        }, true, 100, 100)
            .await
            .unwrap();
        insta::assert_snapshot!("format_output_multiline_ansi_preserved", actual_preserved);

        // Test with ANSI codes stripped
        let actual_stripped = format_output(&infra, fixture, false, 100, 100)
            .await
            .unwrap();
        insta::assert_snapshot!("format_output_multiline_ansi_stripped", actual_stripped);
    }

    #[tokio::test]
    async fn test_format_output_multiline_edge_cases() {
        let infra = Arc::new(MockInfrastructure::new());

        // Test with only newlines
        let fixture = CommandOutput {
            stdout: "\n\n\n".to_string(),
            stderr: "".to_string(),
            command: "only newlines".into(),
            exit_code: Some(0),
        };
        let actual = format_output(&infra, fixture, false, 100, 100)
            .await
            .unwrap();
        insta::assert_snapshot!("format_output_multiline_only_newlines", actual);

        // Test with no trailing newline
        let fixture = CommandOutput {
            stdout: "line1\nline2\nline3".to_string(), // No trailing newline
            stderr: "".to_string(),
            command: "no trailing newline".into(),
            exit_code: Some(0),
        };
        let actual = format_output(&infra, fixture, false, 100, 100)
            .await
            .unwrap();
        insta::assert_snapshot!("format_output_multiline_no_trailing_newline", actual);
    }

    #[tokio::test]
    async fn test_shell_multiline_output_integration() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);

        // Test a command that would produce multiline output
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: "echo -e 'line1\\nline2\\nline3'".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: false,
                    explanation: None,
                },
            )
            .await
            .unwrap();
        insta::assert_snapshot!("shell_multiline_output_integration", &result.into_string());
    }
}
