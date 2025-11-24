use async_trait::async_trait;

use crate::CommandOutput;

/// Port for command execution operations
///
/// This trait defines the interface for executing shell commands,
/// providing an abstraction over different execution strategies.
#[async_trait]
pub trait CommandExecutor: Send + Sync {
    /// Executes a shell command and returns output
    async fn execute_command(
        &self,
        command: String,
        working_dir: std::path::PathBuf,
        silent: bool,
        env_vars: Option<Vec<String>>,
    ) -> anyhow::Result<CommandOutput>;

    /// Execute shell command on present stdio
    async fn execute_command_raw(
        &self,
        command: &str,
        working_dir: std::path::PathBuf,
        env_vars: Option<Vec<String>>,
    ) -> anyhow::Result<std::process::ExitStatus>;
}
