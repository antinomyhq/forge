use std::path::PathBuf;
use std::sync::Arc;

use anyhow::bail;
use forge_display::TitleFormat;
use forge_domain::{
    CommandOutput, Environment, EnvironmentService, ExecutableTool, NamedTool, ToolCallContext,
    ToolDescription, ToolName,
};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strip_ansi_escapes::strip;

use crate::{CommandExecutorService, Infrastructure};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ShellInput {
    /// The shell command to execute.
    pub command: String,
    /// The working directory where the command should be executed.
    pub cwd: PathBuf,
    /// Whether to preserve ANSI escape codes in the output.
    /// If true, ANSI escape codes will be preserved in the output.
    /// If false (default), ANSI escape codes will be stripped from the output.
    #[serde(default)]
    pub keep_ansi: bool,
}

// Strips out the ansi codes from content.
fn strip_ansi(content: String) -> String {
    String::from_utf8_lossy(&strip(content.as_bytes())).into_owned()
}

/// Formats command output by wrapping non-empty stdout/stderr in XML tags.
/// stderr is commonly used for warnings and progress info, so success is
/// determined by exit status, not stderr presence. Returns Ok(output) on
/// success or Err(output) on failure, with a status message if both streams are
/// empty.
fn format_output(mut output: CommandOutput, keep_ansi: bool) -> anyhow::Result<String> {
    const MAX_OUTPUT_SIZE: usize = 40000;
    const TRUNCATE_SIZE: usize = 20000;

    if !keep_ansi {
        output.stderr = strip_ansi(output.stderr);
        output.stdout = strip_ansi(output.stdout);
    }

    // Check for empty output before adding metadata
    if output.stdout.trim().is_empty() && output.stderr.trim().is_empty() {
        let result = if output.success {
            "Command executed successfully with no output".to_string()
        } else {
            "Command failed with no output".to_string()
        };
        return if output.success {
            Ok(result)
        } else {
            Err(anyhow::anyhow!(result))
        };
    }

    let mut formatted_output = String::new();

    // Add metadata section
    formatted_output.push_str("---\n");
    formatted_output.push_str(&format!("command: {}\n", output.command));
    formatted_output.push_str(&format!("total_stdout_chars: {}\n", output.stdout.len()));
    formatted_output.push_str(&format!("total_stderr_chars: {}\n", output.stderr.len()));

    let is_truncated =
        output.stdout.len() > MAX_OUTPUT_SIZE || output.stderr.len() > MAX_OUTPUT_SIZE;
    if is_truncated {
        formatted_output.push_str("truncated: true\n");
        // Create temp file for full output
        let temp_file = tempfile::Builder::new()
            .prefix("forge_shell_")
            .suffix(".txt")
            .tempfile()?;
        let temp_path = temp_file.path().to_string_lossy().to_string();
        std::fs::write(&temp_path, &output.stdout)?;
        formatted_output.push_str(&format!("temp_file: {temp_path}\n"));
    }
    formatted_output.push_str(&format!(
        "exit_code: {}\n",
        if output.success { 0 } else { 1 }
    ));
    formatted_output.push_str("---\n");

    // Handle stdout
    if !output.stdout.trim().is_empty() {
        if output.stdout.len() > MAX_OUTPUT_SIZE {
            // First portion
            formatted_output.push_str(&format!(
                "<stdout chars=\"0-{}\">\n{}\n</stdout>\n",
                TRUNCATE_SIZE,
                &output.stdout[..TRUNCATE_SIZE]
            ));

            // Truncation message
            let omitted = output.stdout.len() - (2 * TRUNCATE_SIZE);
            formatted_output.push_str(&format!(
                "<truncated>\n...output truncated ({omitted} characters not shown)...\n</truncated>\n"
            ));

            // Last portion
            formatted_output.push_str(&format!(
                "<stdout chars=\"{}-{}\">\n{}\n</stdout>\n",
                output.stdout.len() - TRUNCATE_SIZE,
                output.stdout.len(),
                &output.stdout[output.stdout.len() - TRUNCATE_SIZE..]
            ));
        } else {
            formatted_output.push_str(&format!("<stdout>\n{}\n</stdout>\n", output.stdout));
        }
    }

    // Handle stderr
    if !output.stderr.trim().is_empty() {
        if output.stderr.len() > MAX_OUTPUT_SIZE {
            formatted_output.push_str(&format!(
                "<stderr chars=\"0-{}\">\n{}\n</stderr>\n",
                TRUNCATE_SIZE,
                &output.stderr[..TRUNCATE_SIZE]
            ));

            let omitted = output.stderr.len() - (2 * TRUNCATE_SIZE);
            formatted_output.push_str(&format!(
                "<truncated>\n...output truncated ({omitted} characters not shown)...\n</truncated>\n"
            ));

            formatted_output.push_str(&format!(
                "<stderr chars=\"{}-{}\">\n{}\n</stderr>\n",
                output.stderr.len() - TRUNCATE_SIZE,
                output.stderr.len(),
                &output.stderr[output.stderr.len() - TRUNCATE_SIZE..]
            ));
        } else {
            formatted_output.push_str(&format!("<stderr>\n{}\n</stderr>\n", output.stderr));
        }
    }

    if output.success {
        Ok(formatted_output)
    } else {
        Err(anyhow::anyhow!(formatted_output))
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
}

impl<I> NamedTool for Shell<I> {
    fn tool_name() -> ToolName {
        ToolName::new("forge_tool_process_shell")
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> ExecutableTool for Shell<I> {
    type Input = ShellInput;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
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

        format_output(output, input.keep_ansi)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::{env, fs};

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;

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

        assert!(result.contains("---"));
        assert!(result.contains("command:"));
        assert!(result.contains("total_stdout_chars:"));
        assert!(result.contains("total_stderr_chars:"));
        assert!(result.contains("exit_code: 0"));
        assert!(result.contains("<stdout>"));
        assert!(result.contains("to stdout"));
        assert!(result.contains("</stdout>"));
        assert!(result.contains("<stderr>"));
        assert!(result.contains("to stderr"));
        assert!(result.contains("</stderr>"));
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

        assert!(result.contains("---"));
        assert!(result.contains("command:"));
        assert!(result.contains("total_stdout_chars:"));
        assert!(result.contains("total_stderr_chars:"));
        assert!(result.contains("exit_code: 0"));
        assert!(result.contains("<stdout>"));
        assert!(result.contains("to stdout"));
        assert!(result.contains("</stdout>"));
        assert!(result.contains("<stderr>"));
        assert!(result.contains("to stderr"));
        assert!(result.contains("</stderr>"));
    }

    #[tokio::test]
    async fn test_shell_with_working_directory() {
        let infra = Arc::new(MockInfrastructure::new());
        let shell = Shell::new(infra);
        let temp_dir = fs::canonicalize(env::temp_dir()).unwrap();

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

        assert!(result.contains("---"));
        assert!(result.contains("command:"));
        assert!(result.contains("total_stdout_chars:"));
        assert!(result.contains("total_stderr_chars:"));
        assert!(result.contains("exit_code: 0"));
        assert!(result.contains("<stdout>"));
        assert!(result.contains(&temp_dir.display().to_string()));
        assert!(result.contains("</stdout>"));
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

        assert!(result.contains("---"));
        assert!(result.contains("command:"));
        assert!(result.contains("total_stdout_chars:"));
        assert!(result.contains("total_stderr_chars:"));
        assert!(result.contains("exit_code: 0"));
        assert!(result.contains("<stdout>"));
        assert!(result.contains(&current_dir.display().to_string()));
        assert!(result.contains("</stdout>"));
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

        assert!(result.contains("---"));
        assert!(result.contains("command:"));
        assert!(result.contains("total_stdout_chars:"));
        assert!(result.contains("total_stderr_chars:"));
        assert!(result.contains("exit_code: 0"));
        assert!(result.contains("<stdout>"));
        assert!(result.contains("first"));
        assert!(result.contains("second"));
        assert!(result.contains("</stdout>"));
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

        assert!(result.contains("Command executed successfully with no output"));
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

        assert!(result.contains("Command executed successfully with no output"));
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

        assert!(result.contains("---"));
        assert!(result.contains("command:"));
        assert!(result.contains("total_stdout_chars:"));
        assert!(result.contains("total_stderr_chars:"));
        assert!(result.contains("exit_code: 0"));
        assert!(result.contains("<stdout>"));
        assert!(result.contains("/usr/bin:/bin:/usr/sbin:/sbin"));
        assert!(result.contains("</stdout>"));
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

    #[test]
    fn test_format_output_ansi_handling() {
        // Test with keep_ansi = true (should preserve ANSI codes)
        let ansi_output = CommandOutput {
            stdout: "\x1b[32mSuccess\x1b[0m".to_string(),
            stderr: "\x1b[31mWarning\x1b[0m".to_string(),
            success: true,
            command: "echo test".to_string(),
        };
        let preserved = format_output(ansi_output, true).unwrap();
        assert!(preserved.contains("---"));
        assert!(preserved.contains("command: echo test"));
        assert!(preserved.contains("total_stdout_chars:"));
        assert!(preserved.contains("total_stderr_chars:"));
        assert!(preserved.contains("exit_code: 0"));
        assert!(preserved.contains("<stdout>"));
        assert!(preserved.contains("\x1b[32mSuccess\x1b[0m"));
        assert!(preserved.contains("</stdout>"));
        assert!(preserved.contains("<stderr>"));
        assert!(preserved.contains("\x1b[31mWarning\x1b[0m"));
        assert!(preserved.contains("</stderr>"));

        // Test with keep_ansi = false (should strip ANSI codes)
        let ansi_output = CommandOutput {
            stdout: "\x1b[32mSuccess\x1b[0m".to_string(),
            stderr: "\x1b[31mWarning\x1b[0m".to_string(),
            success: true,
            command: "echo test".to_string(),
        };
        let stripped = format_output(ansi_output, false).unwrap();
        assert!(stripped.contains("---"));
        assert!(stripped.contains("command: echo test"));
        assert!(stripped.contains("total_stdout_chars:"));
        assert!(stripped.contains("total_stderr_chars:"));
        assert!(stripped.contains("exit_code: 0"));
        assert!(stripped.contains("<stdout>"));
        assert!(stripped.contains("Success"));
        assert!(stripped.contains("</stdout>"));
        assert!(stripped.contains("<stderr>"));
        assert!(stripped.contains("Warning"));
        assert!(stripped.contains("</stderr>"));
    }

    #[test]
    fn test_format_output_large_output() {
        // Create a large output string
        let large_output = "a".repeat(50000);
        let output = CommandOutput {
            stdout: large_output.clone(),
            stderr: "".to_string(),
            success: true,
            command: "echo large".to_string(),
        };

        let result = format_output(output, false).unwrap();

        // Check metadata
        assert!(result.contains("total_stdout_chars: 50000"));
        assert!(result.contains("truncated: true"));
        assert!(result.contains("temp_file:"));

        // Check first portion
        assert!(result.contains("<stdout chars=\"0-20000\">"));
        assert!(result.contains(&"a".repeat(20000)));

        // Check truncation message
        assert!(result.contains("<truncated>"));
        assert!(result.contains("10000 characters not shown"));

        // Check last portion
        assert!(result.contains("<stdout chars=\"30000-50000\">"));
        assert!(result.contains(&"a".repeat(20000)));
    }
}
