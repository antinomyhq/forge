/// Output from a command execution
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}
