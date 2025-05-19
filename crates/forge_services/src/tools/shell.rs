// PathBuf now comes from the ShellInput in forge_domain
use std::sync::Arc;

use anyhow::bail;
use forge_display::TitleFormat;
use forge_domain::{
    CommandOutput, Environment, EnvironmentService, ExecutableTool, NamedTool, ShellInput,
    ToolCallContext, ToolDescription, ToolName, ToolOutput,
};
use forge_tool_macros::ToolDescription;
use strip_ansi_escapes::strip;

use crate::metadata::Metadata;
use crate::{Clipper, ClipperResult, CommandExecutorService, FsWriteService, Infrastructure};

/// Number of characters to keep at the start of truncated output
const PREFIX_CHARS: usize = 10_000;

/// Number of characters to keep at the end of truncated output
const SUFFIX_CHARS: usize = 10_000;

// Using ShellInput from forge_domain

// Strips out the ansi codes from content.
fn strip_ansi(content: String) -> String {
    String::from_utf8_lossy(&strip(content.as_bytes())).into_owned()
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
    prefix_chars: usize,
    suffix_chars: usize,
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
        let result = Clipper::from_start_end(prefix_chars, suffix_chars).clip(&output.stdout);

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
        let result = Clipper::from_start_end(prefix_chars, suffix_chars).clip(&output.stderr);

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
        context: ToolCallContext,
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
            PREFIX_CHARS,
            SUFFIX_CHARS,
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
                ToolCallContext::default(),
                ShellInput {
                    command: "echo 'Hello, World!'".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: if cfg!(target_os = "windows") {
                        "echo 'to stderr' 1>&2 && echo 'to stdout'".to_string()
                    } else {
                        "echo 'to stderr' >&2; echo 'to stdout'".to_string()
                    },
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: "echo 'to stdout' && echo 'to stderr' >&2".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: if cfg!(target_os = "windows") {
                        "cd".to_string()
                    } else {
                        "pwd".to_string()
                    },
                    cwd: temp_dir.clone(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: "non_existent_command".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: "".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: if cfg!(target_os = "windows") {
                        "cd".to_string()
                    } else {
                        "pwd".to_string()
                    },
                    cwd: current_dir.clone(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: "echo 'first' && echo 'second'".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: "true".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: "echo ''".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: "echo $PATH".to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
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
                ToolCallContext::default(),
                ShellInput {
                    command: cmd.to_string(),
                    cwd: env::current_dir().unwrap(),
                    keep_ansi: true,
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
        let preserved = format_output(&infra, ansi_output, true, PREFIX_CHARS, SUFFIX_CHARS)
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
        let stripped = format_output(&infra, ansi_output, false, PREFIX_CHARS, SUFFIX_CHARS)
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
