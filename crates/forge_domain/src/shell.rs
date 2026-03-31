use std::path::PathBuf;

/// Output from a command execution
#[derive(Debug, Clone)]
pub struct CommandOutput {
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

/// Output from a background (detached) command execution.
///
/// Wraps a `CommandOutput` with the process ID and the `NamedTempFile` handle
/// that owns the log file on disk. Keeping the handle alive prevents the temp
/// file from being deleted.
#[derive(Debug)]
pub struct BackgroundCommandOutput {
    /// The original command string that was executed.
    pub command: String,
    /// OS process ID of the spawned background process.
    pub pid: u32,
    /// Absolute path to the log file capturing stdout/stderr.
    pub log_file: PathBuf,
    /// The temp-file handle; dropping it deletes the log from disk.
    pub log_handle: tempfile::NamedTempFile,
}
