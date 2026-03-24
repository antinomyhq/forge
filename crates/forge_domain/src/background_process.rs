use std::hash::Hasher;
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

impl BackgroundProcess {
    /// Creates an FNV-64 hash of the CWD path for use as a metadata filename.
    pub fn cwd_hash(cwd: &std::path::Path) -> String {
        let mut hasher = fnv_rs::Fnv64::default();
        hasher.write(cwd.to_string_lossy().as_bytes());
        format!("{:x}", hasher.finish())
    }
}
