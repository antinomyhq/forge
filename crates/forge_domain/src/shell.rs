/// Output from a command execution
#[derive(Debug, Clone)]
pub struct CommandOutput {
    /// The command that was executed
    pub command: String,
    /// Standard output from the command
    pub stdout: String,
    /// Standard error from the command
    pub stderr: String,
    /// Exit code of the command (0 for success, non-zero for failure)
    pub exit_code: Option<i32>,
    /// Path to a temporary file containing the full output (for large outputs)
    pub temp_file_path: Option<String>,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code.is_none_or(|code| code >= 0)
    }
}
