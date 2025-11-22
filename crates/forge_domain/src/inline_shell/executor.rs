use std::path::Path;

use anyhow::Result;

use crate::inline_shell::inline_error::InlineShellError;
use crate::inline_shell::parser::InlineShellCommand;

/// Result of executing an inline shell command
#[derive(Debug, Clone, PartialEq)]
pub struct CommandResult {
    /// The original ![command] match from input
    pub original_match: String,
    /// The command that was executed
    pub command: String,
    /// Standard output from command
    pub stdout: String,
    /// Standard error from command
    pub stderr: String,
    /// Exit code from command
    pub exit_code: i32,
}

impl CommandResult {
    /// Returns replacement text for inline command
    pub fn replacement_text(&self) -> String {
        if self.exit_code == 0 {
            self.stdout.clone()
        } else {
            let error_msg = if self.stderr.trim().is_empty() {
                format!("Command failed with exit code {}", self.exit_code)
            } else {
                format!("Command failed: {}", self.stderr.trim())
            };
            format!("[Error: {}]", error_msg)
        }
    }
}

/// Interface for executing inline shell commands
pub trait InlineShellExecutor: Send + Sync {
    /// Executes multiple inline shell commands sequentially
    fn execute_commands(
        &self,
        commands: Vec<InlineShellCommand>,
        working_dir: &Path,
        unrestricted: bool,
    ) -> impl std::future::Future<Output = Result<Vec<CommandResult>, InlineShellError>> + Send;

    /// Replaces inline commands in content with their execution results
    fn replace_commands_in_content(&self, content: &str, results: &[CommandResult]) -> String;
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_command_result(
        original_match: &str,
        command: &str,
        stdout: &str,
        stderr: &str,
        exit_code: i32,
    ) -> CommandResult {
        CommandResult {
            original_match: original_match.to_string(),
            command: command.to_string(),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            exit_code,
        }
    }

    #[test]
    fn test_replacement_text_success() {
        let result = fixture_command_result(r"![date]", "date", "2025-11-14", "", 0);
        assert_eq!(result.replacement_text(), "2025-11-14");
    }

    #[test]
    fn test_replacement_text_failure_with_stderr() {
        let result = fixture_command_result(r"![ls]", "ls", "", "No such file", 2);
        assert_eq!(result.replacement_text(), "[Error: Command failed: No such file]");
    }

    #[test]
    fn test_replacement_text_failure_no_stderr() {
        let result = fixture_command_result(r"![false]", "false", "", "", 1);
        assert_eq!(result.replacement_text(), "[Error: Command failed with exit code 1]");
    }
}