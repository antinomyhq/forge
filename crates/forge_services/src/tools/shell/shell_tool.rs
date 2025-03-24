use std::path::PathBuf;

use anyhow::bail;
use forge_domain::{Environment, ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::executor::Output;
use crate::tools::shell::executor::CommandExecutor;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ShellInput {
    /// The shell command to execute.
    pub command: String,
    /// The working directory where the command should be executed.
    pub cwd: PathBuf,
}

/// Formats command output by wrapping non-empty stdout/stderr in XML tags.
/// stderr is commonly used for warnings and progress info, so success is
/// determined by exit status, not stderr presence. Returns Ok(output) on
/// success or Err(output) on failure, with a status message if both streams are
/// empty.
fn format_output(output: Output) -> anyhow::Result<String> {
    let mut formatted_output = String::new();

    if !output.stdout.trim().is_empty() {
        formatted_output.push_str(&format!("<stdout>{}</stdout>", output.stdout));
    }

    if !output.stderr.trim().is_empty() {
        if !formatted_output.is_empty() {
            formatted_output.push('\n');
        }
        formatted_output.push_str(&format!("<stderr>{}</stderr>", output.stderr));
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
            provider: Provider::anthropic("test-key"),
            base_path: PathBuf::new(),
            qdrant_key: None,
            qdrant_cluster: None,
            pid: std::process::id(),
            openai_key: None,
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
