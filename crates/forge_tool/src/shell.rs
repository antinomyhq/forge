use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use forge_domain::{NamedTool, ToolCallService, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ShellInput {
    /// The shell command to execute.
    pub command: String,
    /// The working directory where the command should be executed.
    pub cwd: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ShellOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    pub success: bool,
}

/// Formats command output by wrapping non-empty stdout/stderr in XML tags.
/// stderr is commonly used for warnings and progress info, so success is
/// determined by exit status, not stderr presence. Returns Ok(output) on
/// success or Err(output) on failure, with a status message if both streams are
/// empty.
fn format_output(stdout: &str, stderr: &str, success: bool) -> Result<String, String> {
    let mut output = String::new();

    if !stdout.trim().is_empty() {
        output.push_str(&format!("<stdout>{}</stdout>", stdout));
    }

    if !stderr.trim().is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&format!("<stderr>{}</stderr>", stderr));
    }

    let result = if output.is_empty() {
        if success {
            "Command executed successfully with no output.".to_string()
        } else {
            "Command failed with no output.".to_string()
        }
    } else {
        output
    };

    if success {
        Ok(result)
    } else {
        Err(result)
    }
}

/// Execute shell commands with safety checks and validation. This tool provides
/// controlled access to system shell commands while preventing dangerous
/// operations through a comprehensive blacklist and validation system.
#[derive(ToolDescription)]
pub struct Shell {
    blacklist: HashSet<String>,
}

impl Default for Shell {
    fn default() -> Self {
        let mut blacklist = HashSet::new();
        // File System Destruction Commands
        blacklist.insert("rm".to_string());
        blacklist.insert("rmdir".to_string());
        blacklist.insert("del".to_string());

        // Disk/Filesystem Commands
        blacklist.insert("format".to_string());
        blacklist.insert("mkfs".to_string());
        blacklist.insert("dd".to_string());

        // Permission/Ownership Commands
        blacklist.insert("chmod".to_string());
        blacklist.insert("chown".to_string());

        // Privilege Escalation Commands
        blacklist.insert("sudo".to_string());
        blacklist.insert("su".to_string());

        // Code Execution Commands
        blacklist.insert("exec".to_string());
        blacklist.insert("eval".to_string());

        // System Communication Commands
        blacklist.insert("write".to_string());
        blacklist.insert("wall".to_string());

        // System Control Commands
        blacklist.insert("shutdown".to_string());
        blacklist.insert("reboot".to_string());
        blacklist.insert("init".to_string());

        Shell { blacklist }
    }
}

impl Shell {
    fn validate_command(&self, command: &str) -> Result<(), String> {
        let base_command = command
            .split_whitespace()
            .next()
            .ok_or_else(|| "Command string is empty or contains only whitespace".to_string())?;

        if self.blacklist.contains(base_command) {
            return Err(format!("Command '{}' is not allowed", base_command));
        }

        Ok(())
    }
}

impl NamedTool for Shell {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_process_shell")
    }
}

#[async_trait::async_trait]
impl ToolCallService for Shell {
    type Input = ShellInput;

    async fn call(&self, input: Self::Input) -> Result<String, String> {
        self.validate_command(&input.command)?;
        let _ = forge_shell::shell(&input.command).map_err(|e| e.to_string())?;

        /*        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", &input.command]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", &input.command]);
            c
        };

        cmd.current_dir(input.cwd);

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to execute command '{}': {}", input.command, e))?;
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        format_output(&stdout, &stderr, output.status.success())
            */

        Ok("output".to_string())
    }
}

/*#[cfg(test)]
mod tests {
    use std::{env, fs};

    use pretty_assertions::assert_eq;

    use super::*;

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
        let shell = Shell::default();
        let result = shell
            .call(ShellInput {
                command: "echo 'Hello, World!'".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();

        assert!(result.contains("<stdout>Hello, World!</stdout>"));
        assert!(!result.contains("<stderr>"));
    }

    #[tokio::test]
    async fn test_shell_stderr_with_success() {
        let shell = Shell::default();
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

        assert!(result.contains("<stderr>to stderr</stderr>"));
        assert!(result.contains("<stdout>to stdout</stdout>"));
    }

    #[tokio::test]
    async fn test_shell_both_streams() {
        let shell = Shell::default();
        let result = shell
            .call(ShellInput {
                command: "echo 'to stdout' && echo 'to stderr' >&2".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();

        assert!(result.contains("<stdout>to stdout</stdout>"));
        assert!(result.contains("<stderr>to stderr</stderr>"));
    }

    #[tokio::test]
    async fn test_shell_with_working_directory() {
        let shell = Shell::default();
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

        let path_str = result
            .trim()
            .trim_start_matches("<stdout>")
            .trim_end_matches("</stdout>");

        let output_path = PathBuf::from(path_str);
        let actual_path = match fs::canonicalize(output_path.clone()) {
            Ok(path) => path,
            Err(_) => output_path,
        };
        let expected_path = temp_dir.as_path();

        assert_eq!(
            actual_path, expected_path,
            "\nExpected path: {:?}\nActual path: {:?}",
            expected_path, actual_path
        );
    }

    #[tokio::test]
    async fn test_shell_invalid_command() {
        let shell = Shell::default();
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
            .any(|&pattern| err.contains(pattern));

        assert!(
            matches_pattern,
            "Error message '{}' did not match any expected patterns for this platform: {:?}",
            err, COMMAND_NOT_FOUND_PATTERNS
        );
    }

    #[tokio::test]
    async fn test_shell_blacklisted_command() {
        let shell = Shell::default();
        let result = shell
            .call(ShellInput {
                command: "rm -rf /".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_shell_empty_command() {
        let shell = Shell::default();
        let result = shell
            .call(ShellInput { command: "".to_string(), cwd: env::current_dir().unwrap() })
            .await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Command string is empty or contains only whitespace"
        );
    }

    #[tokio::test]
    async fn test_shell_pwd() {
        let shell = Shell::default();
        let current_dir = fs::canonicalize(env::current_dir().unwrap()).unwrap();
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

        let path_str = result
            .trim()
            .trim_start_matches("<stdout>")
            .trim_end_matches("</stdout>");

        let output_path = PathBuf::from(path_str);
        let actual_path = match fs::canonicalize(output_path.clone()) {
            Ok(path) => path,
            Err(_) => output_path,
        };
        assert_eq!(actual_path, current_dir);
    }

    #[tokio::test]
    async fn test_shell_multiple_commands() {
        let shell = Shell::default();
        let result = shell
            .call(ShellInput {
                command: "echo 'first' && echo 'second'".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();

        assert!(result.contains("first"));
        assert!(result.contains("second"));
    }

    #[tokio::test]
    async fn test_shell_empty_output() {
        let shell = Shell::default();
        let result = shell
            .call(ShellInput {
                command: "true".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();

        assert_eq!(result, "Command executed successfully with no output.");
    }

    #[tokio::test]
    async fn test_shell_whitespace_only_output() {
        let shell = Shell::default();
        let result = shell
            .call(ShellInput {
                command: "echo ''".to_string(),
                cwd: env::current_dir().unwrap(),
            })
            .await
            .unwrap();

        assert_eq!(result, "Command executed successfully with no output.");
    }

    #[tokio::test]
    async fn test_shell_with_environment_variables() {
        let shell = Shell::default();
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

    #[test]
    fn test_description() {
        assert!(Shell::default().description().len() > 100)
    }
}
*/