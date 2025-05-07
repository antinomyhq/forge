use std::path::PathBuf;
use std::time::Duration;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{Provider, RetryConfig};

/// Update frequency options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateFrequency {
    /// Check for updates hourly
    Hourly,
    /// Check for updates daily (default)
    #[serde(alias = "daily")]
    Daily,
    /// Check for updates weekly
    Weekly,
    /// Never check for updates automatically
    Never,
}

impl Default for UpdateFrequency {
    fn default() -> Self {
        Self::Daily
    }
}

impl UpdateFrequency {
    /// Get the duration between update checks
    pub fn to_duration(&self) -> Duration {
        match self {
            Self::Hourly => Duration::from_secs(60 * 60),
            Self::Daily => Duration::from_secs(24 * 60 * 60),
            Self::Weekly => Duration::from_secs(7 * 24 * 60 * 60),
            Self::Never => Duration::from_secs(u64::MAX),
        }
    }
}

/// Configuration for application updates
#[derive(Debug, Setters, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
#[setters(strip_option)]
pub struct UpdateConfig {
    /// How frequently to check for updates
    #[serde(default)]
    pub check_frequency: UpdateFrequency,

    /// Whether to automatically update without prompting
    #[serde(default)]
    pub auto_update: bool,
}

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
    /// Configuration for application updates
    #[serde(default)]
    pub update_config: UpdateConfig,
    /// Path to the loaded .env file, if any
    #[serde(default)]
    pub dotenv_path: Option<PathBuf>,
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

    pub fn update_timestamp_path(&self) -> PathBuf {
        self.base_path.join(".update_timestamp")
    }
}
