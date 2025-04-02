use std::path::PathBuf;

use anyhow::bail;
use forge_domain::{Environment, ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::executor::Output;
use crate::range_handler::{
    count_lines, extract_line_range, write_to_temp_file, DEFAULT_LINE_LIMIT,
};
use crate::tools::shell::executor::CommandExecutor;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ShellInput {
    /// The shell command to execute.
    pub command: String,
    /// The working directory where the command should be executed.
    pub cwd: PathBuf,
}

/// Formats command output by wrapping non-empty stdout/stderr in XML tags.
/// Handles large outputs by truncating and adding metadata about displayed
/// range. For large outputs, stores the complete output in a temporary file and
/// includes the file path in the response. By default, displays the last part
/// of stdout/stderr instead of the first part.
/// stderr is commonly used for warnings and progress info, so success is
/// determined by exit status, not stderr presence. Returns Ok(output) on
/// success or Err(output) on failure, with a status message if both streams are
/// empty.
fn format_output(output: Output) -> anyhow::Result<String> {
    let mut formatted_output = String::new();

    // Process stdout if not empty
    if !output.stdout.trim().is_empty() {
        // Count total lines in stdout
        let total_lines = count_lines(&output.stdout);

        if total_lines > DEFAULT_LINE_LIMIT {
            // Write full stdout to a temp file
            let temp_file_info = write_to_temp_file(&output.stdout, "shell_stdout")?;

            // For large output, show the last DEFAULT_LINE_LIMIT lines instead of first
            let start = total_lines
                .saturating_sub(DEFAULT_LINE_LIMIT)
                .saturating_add(1);
            let end = total_lines;

            // Extract the content for that range
            let range_content = extract_line_range(&output.stdout, start, end)?;

            // Add metadata XML tag for truncated stdout with file path
            formatted_output.push_str(&format!(
                "<stdout displayed-range=\"{}-{}\" total-lines=\"{}\" complete-log-output=\"{}\">{}",
                start, end, total_lines, temp_file_info.path, range_content
            ));

            // Close the tag
            formatted_output.push_str("</stdout>");
        } else {
            // For small stdout, include all content
            formatted_output.push_str(&format!("<stdout>{}</stdout>", output.stdout));
        }
    }

    // Process stderr if not empty
    if !output.stderr.trim().is_empty() {
        if !formatted_output.is_empty() {
            formatted_output.push('\n');
        }

        // Count total lines in stderr
        let total_lines = count_lines(&output.stderr);

        if total_lines > DEFAULT_LINE_LIMIT {
            // Write full stderr to a temp file
            let temp_file_info = write_to_temp_file(&output.stderr, "shell_stderr")?;

            // For large output, show the last DEFAULT_LINE_LIMIT lines instead of first
            let start = total_lines
                .saturating_sub(DEFAULT_LINE_LIMIT)
                .saturating_add(1);
            let end = total_lines;

            // Extract the content for that range
            let range_content = extract_line_range(&output.stderr, start, end)?;

            // Add metadata XML tag for truncated stderr with file path
            formatted_output.push_str(&format!(
                "<stderr displayed-range=\"{}-{}\" total-lines=\"{}\" complete-log-output=\"{}\">{}",
                start, end, total_lines, temp_file_info.path, range_content
            ));

            // Close the tag
            formatted_output.push_str("</stderr>");
        } else {
            // For small stderr, include all content
            formatted_output.push_str(&format!("<stderr>{}</stderr>", output.stderr));
        }
    }

    let result = if formatted_output.is_empty() {
        if output.success {
            "Command executed successfully with no output.".to_string()
        } else {
            "Command failed with no output.".to_string()
        }
    } else {
        formatted_output
    };

    if output.success {
        Ok(result)
    } else {
        Err(anyhow::anyhow!(result))
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
pub struct Shell {
    env: Environment,
}

impl Shell {
    /// Create a new Shell with environment configuration
    pub fn new(env: Environment) -> Self {
        Self { env }
    }
}

impl NamedTool for Shell {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_process_shell")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for Shell {
    type Input = ShellInput;

    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
        // Validate empty command
        if input.command.trim().is_empty() {
            bail!("Command string is empty or contains only whitespace".to_string());
        }

        let parameter = if cfg!(target_os = "windows") {
            "/C"
        } else {
            "-c"
        };

        #[cfg(not(test))]
        {
            use forge_display::TitleFormat;

            println!(
                "\n{}",
                TitleFormat::execute(format!(
                    "{} {} {}",
                    self.env.shell, parameter, &input.command
                ))
                .format()
            );
        }

        let mut command = Command::new(&self.env.shell);

        command.args([parameter, &input.command]);

        // Set the current working directory for the command
        command.current_dir(input.cwd);
        // Kill the command when the handler is dropped
        command.kill_on_drop(true);

        format_output(CommandExecutor::new(command).colored().execute().await?)
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use forge_domain::Provider;
    use pretty_assertions::assert_eq;

    use super::*;

    /// Create a default test environment
    fn test_env() -> Environment {
        Environment {
            os: std::env::consts::OS.to_string(),
            cwd: std::env::current_dir().unwrap_or_default(),
            home: Some("/home/user".into()),
            shell: if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            },
            base_path: PathBuf::new(),
            pid: std::process::id(),
            provider: Provider::anthropic("test-key"),
        }
    }

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
        let shell = Shell::new(test_env());
        let result = shell
            .call(ShellInput {
                command: "echo 'Hello, World!'".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();
        assert!(result.contains("<stdout>Hello, World!\n</stdout>"));
    }

    #[tokio::test]
    async fn test_shell_stderr_with_success() {
        let shell = Shell::new(test_env());
        // Use a command that writes to both stdout and stderr
        let result = shell
            .call(ShellInput {
                command: if cfg!(target_os = "windows") {
                    "echo 'to stderr' 1>&2 && echo 'to stdout'".to_string()
                } else {
                    "echo 'to stderr' >&2; echo 'to stdout'".to_string()
                },
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();

        assert_eq!(
            result,
            "<stdout>to stdout\n</stdout>\n<stderr>to stderr\n</stderr>"
        );
    }

    #[tokio::test]
    async fn test_shell_both_streams() {
        let shell = Shell::new(test_env());
        let result = shell
            .call(ShellInput {
                command: "echo 'to stdout' && echo 'to stderr' >&2".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();

        assert_eq!(
            result,
            "<stdout>to stdout\n</stdout>\n<stderr>to stderr\n</stderr>"
        );
    }

    #[tokio::test]
    async fn test_shell_with_working_directory() {
        let shell = Shell::new(test_env());
        let temp_dir = fs::canonicalize(env::temp_dir()).unwrap();

        let result = shell
            .call(ShellInput {
                command: if cfg!(target_os = "windows") {
                    "cd".to_string()
                } else {
                    "pwd".to_string()
                },
                cwd: temp_dir.clone(),
            })
            .await
            .unwrap();
        assert_eq!(result, format!("<stdout>{}\n</stdout>", temp_dir.display()));
    }

    #[tokio::test]
    async fn test_shell_invalid_command() {
        let shell = Shell::new(test_env());
        let result = shell
            .call(ShellInput {
                command: "non_existent_command".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();

        // Check if any of the platform-specific patterns match
        let matches_pattern = COMMAND_NOT_FOUND_PATTERNS
            .iter()
            .any(|&pattern| err.to_string().contains(pattern));

        assert!(
            matches_pattern,
            "Error message '{}' did not match any expected patterns for this platform: {:?}",
            err, COMMAND_NOT_FOUND_PATTERNS
        );
    }

    #[tokio::test]
    async fn test_shell_empty_command() {
        let shell = Shell::new(test_env());
        let result = shell
            .call(ShellInput { command: "".to_string(), cwd: env::current_dir().unwrap() })
            .await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Command string is empty or contains only whitespace"
        );
    }

    #[tokio::test]
    async fn test_description() {
        assert!(Shell::new(test_env()).description().len() > 100)
    }

    #[tokio::test]
    async fn test_shell_pwd() {
        let shell = Shell::new(test_env());
        let current_dir = env::current_dir().unwrap();
        let result = shell
            .call(ShellInput {
                command: if cfg!(target_os = "windows") {
                    "cd".to_string()
                } else {
                    "pwd".to_string()
                },
                cwd: current_dir.clone(),
            })
            .await
            .unwrap();

        assert_eq!(
            result,
            format!("<stdout>{}\n</stdout>", current_dir.display())
        );
    }

    #[tokio::test]
    async fn test_shell_multiple_commands() {
        let shell = Shell::new(test_env());
        let result = shell
            .call(ShellInput {
                command: "echo 'first' && echo 'second'".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();
        assert_eq!(result, format!("<stdout>first\nsecond\n</stdout>"));
    }

    #[tokio::test]
    async fn test_shell_empty_output() {
        let shell = Shell::new(test_env());
        let result = shell
            .call(ShellInput {
                command: "true".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();

        assert!(result.contains("executed successfully"));
        assert!(!result.contains("failed"));
    }

    #[tokio::test]
    async fn test_shell_whitespace_only_output() {
        let shell = Shell::new(test_env());
        let result = shell
            .call(ShellInput {
                command: "echo ''".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();

        assert!(result.contains("executed successfully"));
        assert!(!result.contains("failed"));
    }

    #[tokio::test]
    async fn test_shell_large_output() {
        let shell = Shell::new(test_env());

        // Generate significantly more lines than DEFAULT_LINE_LIMIT
        let num_lines = DEFAULT_LINE_LIMIT * 2;
        let command = if cfg!(target_os = "windows") {
            // Windows command to generate a large output
            format!("for /L %%i in (1,1,{}) do @echo Line %%i", num_lines)
        } else {
            // Unix command to generate a large output
            format!("for i in $(seq 1 {}); do echo Line $i; done", num_lines)
        };

        let result = shell
            .call(ShellInput { command, cwd: env::current_dir().unwrap() })
            .await
            .unwrap();

        // The output should be truncated and include range metadata
        assert!(result.contains("displayed-range"));
        assert!(result.contains("total-lines"));
        assert!(result.contains("complete-log-output"));

        // Check that we're showing the LAST part of the output now
        let start = num_lines - DEFAULT_LINE_LIMIT + 1;
        let end = num_lines;
        assert!(result.contains(&format!(
            "<stdout displayed-range=\"{}-{}\" total-lines=\"{}\"",
            start, end, num_lines
        )));
        // Verify the content includes the last lines but not lines before the range
        assert!(!result.contains("Line 1\n"));
        assert!(result.contains(&format!("Line {}", DEFAULT_LINE_LIMIT + 1)));
        assert!(result.contains(&format!("Line {}", num_lines)));

        // Verify the log file path is included and exists
        let path_start =
            result.find("complete-log-output=\"").unwrap() + "complete-log-output=\"".len();
        let path_end = result[path_start..].find("\"").unwrap() + path_start;
        let log_path = &result[path_start..path_end];

        assert!(std::path::Path::new(log_path).exists());
    }

    #[tokio::test]
    async fn test_shell_large_stderr_output() {
        let shell = Shell::new(test_env());

        // Generate significantly more lines than DEFAULT_LINE_LIMIT
        let num_lines = DEFAULT_LINE_LIMIT * 2;
        let command = if cfg!(target_os = "windows") {
            // Windows command to generate large stderr output with successful exit code
            format!(
                "for /L %%i in (1,1,{}) do @echo Error Line %%i 1>&2 && exit /b 0",
                num_lines
            )
        } else {
            // Unix command to generate large stderr output with successful exit code
            format!(
                "for i in $(seq 1 {}); do echo \"Error Line $i\" >&2; done && exit 0",
                num_lines
            )
        };

        let result = shell
            .call(ShellInput { command, cwd: env::current_dir().unwrap() })
            .await
            .unwrap();

        // The stderr output should be truncated and include range metadata
        assert!(result.contains("displayed-range"));
        assert!(result.contains("total-lines"));
        assert!(result.contains("complete-log-output"));

        // Check that we're showing the LAST part of the output now
        let start = num_lines - DEFAULT_LINE_LIMIT + 1;
        let end = num_lines;
        assert!(result.contains(&format!(
            "<stderr displayed-range=\"{}-{}\" total-lines=\"{}\"",
            start, end, num_lines
        )));

        // Verify the content includes the last lines but not lines before the range
        assert!(!result.contains("Error Line 1\n"));
        assert!(result.contains(&format!("Error Line {}", DEFAULT_LINE_LIMIT + 1)));
        assert!(result.contains(&format!("Error Line {}", num_lines)));

        // Verify the log file path is included and exists
        let path_start =
            result.find("complete-log-output=\"").unwrap() + "complete-log-output=\"".len();
        let path_end = result[path_start..].find("\"").unwrap() + path_start;
        let log_path = &result[path_start..path_end];

        assert!(std::path::Path::new(log_path).exists());
    }

    #[tokio::test]
    async fn test_shell_with_environment_variables() {
        let shell = Shell::new(test_env());
        let result = shell
            .call(ShellInput {
                command: "echo $PATH".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();

        assert!(!result.is_empty());
        assert!(!result.contains("Error:"));
    }

    #[tokio::test]
    async fn test_shell_full_path_command() {
        let shell = Shell::new(test_env());
        // Using a full path command which would be restricted in rbash
        let cmd = if cfg!(target_os = "windows") {
            r"C:\Windows\System32\whoami.exe"
        } else {
            "/bin/ls"
        };

        let result = shell
            .call(ShellInput { command: cmd.to_string(), cwd: env::current_dir().unwrap() })
            .await;

        // In rbash, this would fail with a permission error
        // For our normal shell test, it should succeed
        assert!(
            result.is_ok(),
            "Full path commands should work in normal shell"
        );
    }
}
