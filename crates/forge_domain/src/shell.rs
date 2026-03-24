/// Output from a command execution
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub pid: Option<u32>,
    pub log_path: Option<String>,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code.is_none_or(|code| code >= 0)
    }
}
