// PathBuf now comes from the ShellInput in forge_domain
use std::sync::Arc;

use anyhow::bail;
use forge_display::TitleFormat;
use forge_domain::{
    CommandOutput, Environment, ExecutableTool, NamedTool, ShellInput, ToolCallContext,
    ToolDescription, ToolName, ToolOutput,
};
use forge_tool_macros::ToolDescription;
use strip_ansi_escapes::strip;

use crate::metadata::Metadata;
use crate::services::EnvironmentService;
use crate::{ClipperResult, CommandExecutorService, FsWriteService, Infrastructure};

// Strips out the ansi codes from content.
fn strip_ansi(content: String) -> String {
    String::from_utf8_lossy(&strip(content.as_bytes())).into_owned()
}

/// Number of lines to keep at the start of truncated output
const PREFIX_LINES: usize = 1_000;

/// Number of lines to keep at the end of truncated output
const SUFFIX_LINES: usize = 1_000;

// Using ShellInput from forge_domain

/// Clips text content based on line count instead of character count
fn clip_by_lines(content: &str, prefix_lines: usize, suffix_lines: usize) -> ClipperResult<'_> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // If content fits within limits, return it as-is
    if total_lines <= prefix_lines + suffix_lines {
        return ClipperResult { prefix: None, suffix: None, actual: content };
    }

    // Calculate character positions for line-based ranges
    let mut char_pos = 0;
    let mut prefix_end_char = 0;
    let mut suffix_start_char = 0;

    // Find character position for end of prefix lines
    for (i, line) in lines.iter().enumerate() {
        if i < prefix_lines {
            char_pos += line.len();
            if i < lines.len() - 1 {
                // Add newline except for last line
                char_pos += 1;
            }
            prefix_end_char = char_pos;
        } else if i >= total_lines - suffix_lines {
            if suffix_start_char == 0 {
                suffix_start_char = char_pos;
            }
            break;
        } else {
            char_pos += line.len() + 1; // Include newline
        }
    }

    // Calculate suffix start position
    if suffix_start_char == 0 {
        char_pos = 0;
        for (i, line) in lines.iter().enumerate() {
            if i >= total_lines - suffix_lines {
                suffix_start_char = char_pos;
                break;
            }
            char_pos += line.len() + 1;
        }
    }

    ClipperResult {
        prefix: Some(0..prefix_end_char),
        suffix: Some(suffix_start_char..content.len()),
        actual: content,
    }
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
    let mut formatted_output = String::new();

    if !keep_ansi {
        output.stderr = strip_ansi(output.stderr);
        output.stdout = strip_ansi(output.stdout);
    }

    // Create metadata
    let mut metadata = Metadata::default()
        .add("command", &output.command)
        .add_optional("exit_code", output.exit_code);

    let mut is_truncated = false;

    // Format stdout if not empty
    if !output.stdout.trim().is_empty() {
        let result = clip_by_lines(&output.stdout, prefix_lines, suffix_lines);

        if result.is_truncated() {
            metadata = metadata.add("total_stdout_chars", output.stdout.len());
            is_truncated = true;
        }
        formatted_output.push_str(&tag_output(result, "stdout", &output.stdout));
    }

    // Format stderr if not empty
    if !output.stderr.trim().is_empty() {
        if !formatted_output.is_empty() {
            formatted_output.push('\n');
        }
        let result = clip_by_lines(&output.stderr, prefix_lines, suffix_lines);

        if result.is_truncated() {
            metadata = metadata.add("total_stderr_chars", output.stderr.len());
            is_truncated = true;
        }
        formatted_output.push_str(&tag_output(result, "stderr", &output.stderr));
    }

    // Add temp file path if output is truncated
    if is_truncated {
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
        formatted_output.push_str(&format!(
            "<truncate>content is truncated, remaining content can be read from path:{}</truncate>",
            path.display()
        ));
    }

    // Handle empty outputs
    let result = if formatted_output.is_empty() {
        if output.success() {
            "Command executed successfully with no output.".to_string()
        } else {
            "Command failed with no output.".to_string()
        }
    } else {
        formatted_output
    };

    if output.success() {
        Ok(format!("{metadata}{result}"))
    } else {
        bail!(format!("{metadata}{result}"))
    }
}

/// Helper function to format potentially truncated output for stdout or stderr
fn tag_output(result: ClipperResult, tag: &str, content: &str) -> String {
    let mut formatted_output = String::default();
    match (result.prefix, result.suffix) {
        (Some(prefix), Some(suffix)) => {
            let truncated_chars = content.len() - prefix.len() - suffix.len();
            let prefix_content = &content[prefix.clone()];
            let suffix_content = &content[suffix.clone()];

            formatted_output.push_str(&format!(
                "<{} chars=\"{}-{}\">\n{}\n</{}>\n",
                tag, prefix.start, prefix.end, prefix_content, tag
            ));
            formatted_output.push_str(&format!(
                "<truncated>...{tag} truncated ({truncated_chars} characters not shown)...</truncated>\n"
            ));
            formatted_output.push_str(&format!(
                "<{} chars=\"{}-{}\">\n{}\n</{}>\n",
                tag, suffix.start, suffix.end, suffix_content, tag
            ));
        }
        _ => formatted_output.push_str(&format!("<{tag}>\n{content}\n</{tag}>")),
    }

    formatted_output
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
        // Validate empty command
        if input.command.trim().is_empty() {
            bail!("Command string is empty or contains only whitespace".to_string());
        }
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
        let actual = clip_by_lines(fixture, 5, 5);
        let expected = ClipperResult { prefix: None, suffix: None, actual: fixture };
        assert_eq!(actual.prefix, expected.prefix);
        assert_eq!(actual.suffix, expected.suffix);
        assert_eq!(actual.actual, expected.actual);
        assert_eq!(actual.is_truncated(), false);
    }

    #[test]
    fn test_clip_by_lines_with_truncation() {
        let fixture = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8";
        let actual = clip_by_lines(fixture, 2, 2);

        // Should have both prefix and suffix ranges
        assert!(actual.is_truncated());
        assert!(actual.prefix.is_some());
        assert!(actual.suffix.is_some());

        let prefix_content = actual.prefix_content().unwrap();
        let suffix_content = actual.suffix_content().unwrap();

        assert_eq!(prefix_content, "line1\nline2\n");
        assert_eq!(suffix_content, "line7\nline8");
    }

    #[test]
    fn test_clip_by_lines_single_line() {
        let fixture = "single line";
        let actual = clip_by_lines(fixture, 1, 1);
        let expected = ClipperResult { prefix: None, suffix: None, actual: fixture };
        assert_eq!(actual.prefix, expected.prefix);
        assert_eq!(actual.suffix, expected.suffix);
        assert_eq!(actual.is_truncated(), false);
    }

    #[test]
    fn test_clip_by_lines_empty_content() {
        let fixture = "";
        let actual = clip_by_lines(fixture, 5, 5);
        assert_eq!(actual.is_truncated(), false);
        assert_eq!(actual.actual, "");
    }

    #[test]
    fn test_clip_by_lines_exact_boundary() {
        let fixture = "line1\nline2\nline3\nline4";
        let actual = clip_by_lines(fixture, 2, 2);
        // Exactly 4 lines with 2+2 limit should not truncate
        assert_eq!(actual.is_truncated(), false);
    }

    #[test]
    fn test_clip_by_lines_newline_handling() {
        let fixture = "line1\nline2\nline3\nline4\nline5\nline6";
        let actual = clip_by_lines(fixture, 2, 1);

        assert!(actual.is_truncated());
        let prefix_content = actual.prefix_content().unwrap();
        let suffix_content = actual.suffix_content().unwrap();

        assert_eq!(prefix_content, "line1\nline2\n");
        assert_eq!(suffix_content, "line6");
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
        let fixture = "simple output";
        let result = ClipperResult { prefix: None, suffix: None, actual: fixture };
        let actual = tag_output(result, "stdout", fixture);
        let expected = "<stdout>\nsimple output\n</stdout>";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_tag_output_with_truncation() {
        let fixture = "This is a long line that will be truncated";
        let result = ClipperResult {
            prefix: Some(0..8),   // "This is "
            suffix: Some(35..42), // "cated"
            actual: fixture,
        };
        let actual = tag_output(result, "stderr", fixture);

        assert!(actual.contains("<stderr chars=\"0-8\">"));
        assert!(actual.contains("This is "));
        assert!(actual.contains("<stderr chars=\"35-42\">"));
        assert!(actual.contains("cated"));
        assert!(actual.contains("truncated"));
    }

    #[tokio::test]
    async fn test_format_output_empty_stdout_stderr() {
        let infra = Arc::new(MockInfrastructure::new());
        let fixture = CommandOutput {
            stdout: "".to_string(),
            stderr: "".to_string(),
            command: "true".into(),
            exit_code: Some(0),
        };
        let actual = format_output(&infra, fixture, false, 100, 100)
            .await
            .unwrap();
        assert!(actual.contains("Command executed successfully with no output"));
    }

    #[tokio::test]
    async fn test_format_output_empty_with_failure() {
        let infra = Arc::new(MockInfrastructure::new());
        let fixture = CommandOutput {
            stdout: "".to_string(),
            stderr: "".to_string(),
            command: "false".into(),
            exit_code: Some(-1), // Negative exit code indicates failure
        };
        let result = format_output(&infra, fixture, false, 100, 100).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Command failed with no output"));
    }

    #[tokio::test]
    async fn test_format_output_whitespace_only() {
        let infra = Arc::new(MockInfrastructure::new());
        let fixture = CommandOutput {
            stdout: "   \t\n  ".to_string(),
            stderr: "  \n\t  ".to_string(),
            command: "echo".into(),
            exit_code: Some(0),
        };
        let actual = format_output(&infra, fixture, false, 100, 100)
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
        let current_dir = env::current_dir().unwrap();
        let result = shell
            .call(
                &mut ToolCallContext::default(),
                ShellInput {
                    command: if cfg!(target_os = "windows") {
                        "cd".to_string()
                    } else {
                        "pwd".to_string()
                    },
                    cwd: current_dir.clone(),
                    keep_ansi: true,
                    explanation: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(
            result.into_string(),
            format!(
                "{}<stdout>\n{}\n\n</stdout>",
                Metadata::default()
                    .add(
                        "command",
                        if cfg!(target_os = "windows") {
                            "cd"
                        } else {
                            "pwd"
                        }
                    )
                    .add("exit_code", 0)
                    .to_string(),
                current_dir.display()
            )
        );
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
}
