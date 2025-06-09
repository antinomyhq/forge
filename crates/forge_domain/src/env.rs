use std::path::PathBuf;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{Provider, RetryConfig};

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Debug, Setters, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[setters(strip_option)]
/// Represents the environment in which the application is running.
pub struct Environment {
    /// The operating system of the environment.
    pub os: String,
    /// The process ID of the current process.
    pub pid: u32,
    /// The current working directory.
    pub cwd: PathBuf,
    /// The home directory.
    pub home: Option<PathBuf>,
    /// The shell being used.
    pub shell: String,
    /// The base path relative to which everything else stored.
    pub base_path: PathBuf,
    /// Resolved provider based on the environment configuration.    
    pub provider: Provider,
    /// Configuration for the retry mechanism
    pub retry_config: RetryConfig,
}

impl Environment {
    pub fn db_path(&self) -> PathBuf {
        self.base_path.clone()
    }

    pub fn log_path(&self) -> PathBuf {
        self.base_path.join("logs")
    }

    pub fn history_path(&self) -> PathBuf {
        self.base_path.join(".forge_history")
    }
    pub fn snapshot_path(&self) -> PathBuf {
        self.base_path.join("snapshots")
    }
    pub fn mcp_user_config(&self) -> PathBuf {
        self.base_path.join(".mcp.json")
    }

    pub fn mcp_local_config(&self) -> PathBuf {
        self.cwd.join(".mcp.json")
    }
    pub fn version(&self) -> String {
        VERSION.to_string()
    }
    pub fn conversations_dir(&self) -> PathBuf {
        self.base_path.join("conversations")
    }
}
