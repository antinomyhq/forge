use std::fmt::Display;
use std::path::{Path, PathBuf};

use anyhow::Result;
use bytes::Bytes;
use derive_more::Display;
use forge_app::{WalkedFile, Walker};
use forge_domain::{
    CommandOutput, Environment, McpServerConfig, ToolDefinition, ToolName, ToolOutput,
};
use forge_snaps::Snapshot;
use reqwest::header::HeaderMap;
use reqwest::Response;
use strum_macros::EnumIter;

#[derive(Debug, Clone, PartialEq, Display, EnumIter)]
pub enum CommandExecutionPrompt {
    #[display("Accept command")]
    Accept,
    #[display("Reject command")]
    Reject,
    #[display("Accept command (remember)")]
    AcceptAndRemember,
    #[display("Reject command (remember)")]
    RejectAndRemember,
}

impl CommandExecutionPrompt {
    pub fn is_accept(&self) -> bool {
        matches!(
            self,
            CommandExecutionPrompt::Accept | CommandExecutionPrompt::AcceptAndRemember
        )
    }
    pub fn is_remember(&self) -> bool {
        matches!(
            self,
            CommandExecutionPrompt::AcceptAndRemember | CommandExecutionPrompt::RejectAndRemember
        )
    }
}

pub trait EnvironmentInfra: Send + Sync {
    fn get_environment(&self) -> Environment;
    fn get_env_var(&self, key: &str) -> Option<String>;
}

/// Repository for accessing system environment information
/// This uses the EnvironmentService trait from forge_domain
/// A service for reading files from the filesystem.
///
/// This trait provides an abstraction over file reading operations, allowing
/// for both real file system access and test mocking.
#[async_trait::async_trait]
pub trait FileReaderInfra: Send + Sync {
    /// Reads the content of a file at the specified path.
    /// Returns the file content as a UTF-8 string.
    async fn read_utf8(&self, path: &Path) -> anyhow::Result<String>;

    /// Reads the content of a file at the specified path.
    /// Returns the file content as raw bytes.
    async fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>>;

    /// Reads a specific line range from a file at the specified path.
    /// Returns the file content within the range as a UTF-8 string along with
    /// metadata.
    ///
    /// - start_line specifies the starting line position (1-based, inclusive).
    /// - end_line specifies the ending line position (1-based, inclusive).
    /// - Both start_line and end_line are inclusive bounds.
    /// - Binary files are automatically detected and rejected.
    ///
    /// Returns a tuple containing the file content and FileInfo with metadata
    /// about the read operation:
    /// - FileInfo.start_line: starting line position
    /// - FileInfo.end_line: ending line position
    /// - FileInfo.total_lines: total line count in file
    async fn range_read_utf8(
        &self,
        path: &Path,
        start_line: u64,
        end_line: u64,
    ) -> anyhow::Result<(String, forge_fs::FileInfo)>;
}

#[async_trait::async_trait]
pub trait FileWriterInfra: Send + Sync {
    /// Writes the content of a file at the specified path.
    async fn write(
        &self,
        path: &Path,
        contents: Bytes,
        capture_snapshot: bool,
    ) -> anyhow::Result<()>;

    /// Writes content to a temporary file with the given prefix and extension,
    /// and returns its path. The file will be kept (not deleted) after
    /// creation.
    ///
    /// # Arguments
    /// * `prefix` - Prefix for the temporary file name
    /// * `ext` - File extension (e.g. ".txt", ".md")
    /// * `content` - Content to write to the file
    async fn write_temp(&self, prefix: &str, ext: &str, content: &str) -> anyhow::Result<PathBuf>;
}

#[async_trait::async_trait]
pub trait FileRemoverInfra: Send + Sync {
    /// Removes a file at the specified path.
    async fn remove(&self, path: &Path) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait FileInfoInfra: Send + Sync {
    async fn is_file(&self, path: &Path) -> anyhow::Result<bool>;
    async fn exists(&self, path: &Path) -> anyhow::Result<bool>;
    async fn file_size(&self, path: &Path) -> anyhow::Result<u64>;
}

#[async_trait::async_trait]
pub trait FileDirectoryInfra {
    async fn create_dirs(&self, path: &Path) -> anyhow::Result<()>;
}

/// Service for managing file snapshots
#[async_trait::async_trait]
pub trait SnapshotInfra: Send + Sync {
    // Creation
    async fn create_snapshot(&self, file_path: &Path) -> Result<Snapshot>;

    /// Restores the most recent snapshot for the given file path
    async fn undo_snapshot(&self, file_path: &Path) -> Result<()>;
}

/// Service for executing shell commands
#[async_trait::async_trait]
pub trait CommandInfra: Send + Sync {
    /// Executes a shell command and returns the output
    async fn execute_command(
        &self,
        command: String,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput>;

    /// execute the shell command on present stdio.
    async fn execute_command_raw(&self, command: &str) -> anyhow::Result<std::process::ExitStatus>;
}

#[async_trait::async_trait]
pub trait UserInfra: Send + Sync {
    /// Prompts the user with question
    /// Returns None if the user interrupts the prompt
    async fn prompt_question(&self, question: &str) -> anyhow::Result<Option<String>>;

    /// Prompts the user to select a single option from a list
    /// Returns None if the user interrupts the selection
    async fn select_one<T: Display + Send + Clone + 'static>(
        &self,
        message: &str,
        options: Vec<T>,
    ) -> anyhow::Result<Option<T>>;

    /// Prompts the user to select multiple options from a list
    /// Returns None if the user interrupts the selection
    async fn select_many<T: Display + Send + Clone + 'static>(
        &self,
        message: &str,
        options: Vec<T>,
    ) -> anyhow::Result<Option<Vec<T>>>;
}

#[async_trait::async_trait]
pub trait McpClientInfra: Clone + Send + Sync + 'static {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>>;
    async fn call(
        &self,
        tool_name: &ToolName,
        input: serde_json::Value,
    ) -> anyhow::Result<ToolOutput>;
}

#[async_trait::async_trait]
pub trait McpServerInfra: Send + Sync + 'static {
    type Client: McpClientInfra;
    async fn connect(&self, config: McpServerConfig) -> anyhow::Result<Self::Client>;
}
/// Service for walking filesystem directories
#[async_trait::async_trait]
pub trait WalkerInfra: Send + Sync {
    /// Walks the filesystem starting from the given directory with the
    /// specified configuration
    async fn walk(&self, config: Walker) -> anyhow::Result<Vec<WalkedFile>>;
}

// TODO: rename me, add Infra suffix
#[async_trait::async_trait]
pub trait HttpInfra: Send + Sync + 'static {
    async fn get(&self, url: &str, headers: Option<HeaderMap>) -> anyhow::Result<Response>;
    async fn post(&self, url: &str, body: Bytes) -> anyhow::Result<Response>;
    async fn delete(&self, url: &str) -> anyhow::Result<Response>;
}
