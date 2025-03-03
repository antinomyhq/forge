use std::path::PathBuf;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

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
    /// The Qdrant API Key
    pub qdrant_key: Option<String>,
    /// The Qdrant Cluster
    pub qdrant_cluster: Option<String>,
    /// The base path relative to which everything else stored.
    pub base_path: PathBuf,
    /// The Forge API key.
    pub provider_key: String,
    /// The base url for provider
    pub provider_url: String,
    /// The OpenAI API key required to use embedding models.
    pub openai_key: Option<String>,
    /// The Forge version.
    pub version: String,
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
}
