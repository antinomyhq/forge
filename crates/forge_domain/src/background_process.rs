use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Metadata for a single background process spawned by the shell tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundProcess {
    /// OS process ID.
    pub pid: u32,
    /// The original command string that was executed.
    pub command: String,
    /// Working directory where the command was spawned.
    pub cwd: PathBuf,
    /// Absolute path to the log file capturing stdout/stderr.
    pub log_file: PathBuf,
    /// When the process was spawned.
    pub started_at: DateTime<Utc>,
}
