use std::path::{Path, PathBuf};

use anyhow::Result;
use bytes::Bytes;
use forge_domain::{
    Buffer, CommandOutput, JsonlIterator, McpServerConfig, ToolDefinition, ToolName, ToolOutput,
};
use forge_snaps::Snapshot;

use crate::services::EnvironmentService;

/// Repository for accessing system environment information
/// This uses the EnvironmentService trait from forge_domain
/// A service for reading files from the filesystem.
///
/// This trait provides an abstraction over file reading operations, allowing
/// for both real file system access and test mocking.
#[async_trait::async_trait]
pub trait FsReadService: Send + Sync {
    /// Reads the content of a file at the specified path.
    /// Returns the file content as a UTF-8 string.
    async fn read_utf8(&self, path: &Path) -> anyhow::Result<String>;

    /// Reads the content of a file at the specified path.
    /// Returns the file content as raw bytes.
    async fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>>;

    /// Reads a specific character range from a file at the specified path.
    /// Returns the file content within the range as a UTF-8 string along with
    /// metadata.
    ///
    /// - start_char specifies the starting character position (0-based,
    ///   inclusive).
    /// - end_char specifies the ending character position (inclusive).
    /// - Both start_char and end_char are inclusive bounds.
    /// - Binary files are automatically detected and rejected.
    ///
    /// Returns a tuple containing the file content and FileInfo with metadata
    /// about the read operation:
    /// - FileInfo.start_char: starting character position
    /// - FileInfo.end_char: ending character position
    /// - FileInfo.total_chars: total character count in file
    async fn range_read_utf8(
        &self,
        path: &Path,
        start_char: u64,
        end_char: u64,
    ) -> anyhow::Result<(String, forge_fs::FileInfo)>;
}

#[async_trait::async_trait]
pub trait FsWriteService: Send + Sync {
    /// Writes the content of a file at the specified path.
    async fn write(&self, path: &Path, contents: Bytes) -> anyhow::Result<()>;

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
pub trait FileRemoveService: Send + Sync {
    /// Removes a file at the specified path.
    async fn remove(&self, path: &Path) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait FsMetaService: Send + Sync {
    async fn is_file(&self, path: &Path) -> anyhow::Result<bool>;
    async fn exists(&self, path: &Path) -> anyhow::Result<bool>;
}

#[async_trait::async_trait]
pub trait FsCreateDirsService {
    async fn create_dirs(&self, path: &Path) -> anyhow::Result<()>;
}

/// Service for managing file snapshots
#[async_trait::async_trait]
pub trait FsSnapshotService: Send + Sync {
    // Creation
    async fn create_snapshot(&self, file_path: &Path) -> Result<Snapshot>;

    /// Restores the most recent snapshot for the given file path
    async fn undo_snapshot(&self, file_path: &Path) -> Result<()>;
}

/// Service for executing shell commands
#[async_trait::async_trait]
pub trait CommandExecutorService: Send + Sync {
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
pub trait InquireService: Send + Sync {
    /// Prompts the user with question
    /// Returns None if the user interrupts the prompt
    async fn prompt_question(&self, question: &str) -> anyhow::Result<Option<String>>;

    /// Prompts the user to select a single option from a list
    /// Returns None if the user interrupts the selection
    async fn select_one(
        &self,
        message: &str,
        options: Vec<String>,
    ) -> anyhow::Result<Option<String>>;

    /// Prompts the user to select multiple options from a list
    /// Returns None if the user interrupts the selection
    async fn select_many(
        &self,
        message: &str,
        options: Vec<String>,
    ) -> anyhow::Result<Option<Vec<String>>>;
}

#[async_trait::async_trait]
pub trait McpClient: Send + Sync + 'static {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>>;
    async fn call(
        &self,
        tool_name: &ToolName,
        input: serde_json::Value,
    ) -> anyhow::Result<ToolOutput>;
}

#[async_trait::async_trait]
pub trait McpServer: Send + Sync + 'static {
    type Client: McpClient;
    async fn connect(&self, config: McpServerConfig) -> anyhow::Result<Self::Client>;
}

#[async_trait::async_trait]
pub trait BufferService: Send + Sync + 'static {
    /// Returns an iterator that yields Buffer items from JSONL format
    async fn read_last(&self, path: &Path, n: usize) -> anyhow::Result<JsonlIterator>;

    /// Update the input/output buffer state.
    async fn write(&self, path: &Path, buffer: Buffer) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait ConsolePrintService: Send + Sync + 'static {
    async fn print(&self, output: &str) -> anyhow::Result<()>;
}

pub trait Infrastructure: Send + Sync + Clone + 'static {
    type EnvironmentService: EnvironmentService;
    type FsMetaService: FsMetaService;
    type FsReadService: FsReadService;
    type FsRemoveService: FileRemoveService;
    type FsSnapshotService: FsSnapshotService;
    type FsWriteService: FsWriteService;
    type FsCreateDirsService: FsCreateDirsService;
    type CommandExecutorService: CommandExecutorService;
    type InquireService: InquireService;
    type McpServer: McpServer;
    type BufferService: BufferService;
    type ConsolePrintService: ConsolePrintService;

    fn environment_service(&self) -> &Self::EnvironmentService;
    fn file_meta_service(&self) -> &Self::FsMetaService;
    fn file_read_service(&self) -> &Self::FsReadService;
    fn file_remove_service(&self) -> &Self::FsRemoveService;
    fn file_snapshot_service(&self) -> &Self::FsSnapshotService;
    fn file_write_service(&self) -> &Self::FsWriteService;
    fn create_dirs_service(&self) -> &Self::FsCreateDirsService;
    fn command_executor_service(&self) -> &Self::CommandExecutorService;
    fn inquire_service(&self) -> &Self::InquireService;
    fn mcp_server(&self) -> &Self::McpServer;
    fn buffer_service(&self) -> &Self::BufferService;
    fn console_print_service(&self) -> &Self::ConsolePrintService;
}
