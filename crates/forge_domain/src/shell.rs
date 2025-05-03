/// Output from a command execution
#[derive(Debug, Clone)]
pub struct CommandOutput {
    /// Standard output from the command
    pub stdout: String,
    /// Standard error from the command
    pub stderr: String,
    /// Whether the command executed successfully (based on exit code)
    pub success: bool,
    /// Exit code of the command (0 for success, non-zero for failure)
    pub exit_code: i32,
    /// Path to a temporary file containing the full output (for large outputs)
    pub temp_file_path: Option<String>,
}
