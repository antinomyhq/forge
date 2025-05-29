use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use forge_domain::{
    CommandOutput, Environment, EnvironmentService, McpServerConfig, ToolDefinition, ToolName,
    ToolOutput,
};
use forge_fs::{FileInfo, ForgeFS};
use forge_snaps::{Snapshot, SnapshotId};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::infra::FsUndoService;
use crate::{
    CommandExecutorService, FileRemoveService, FsCreateDirsService, FsMetaService, FsReadService,
    FsSnapshotService, FsWriteService, Infrastructure, McpClient, McpServer,
};

#[derive(Clone)]
pub struct MockInfrastructure {
    temp_dir: Arc<tempfile::TempDir>,
    // Stores command outputs for mocking with the structure we need to recreate CommandOutput
    command_outputs: Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<String, (String, String, String, Option<i32>)>,
        >,
    >,
    // Stores raw command outputs for mocking
    raw_command_outputs:
        Arc<tokio::sync::Mutex<std::collections::HashMap<String, std::process::ExitStatus>>>,
    // Stores custom environment for testing
    environment: Option<Environment>,
}

impl MockInfrastructure {
    pub fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        Self {
            temp_dir: Arc::new(temp_dir),
            command_outputs: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            raw_command_outputs: Arc::new(
                tokio::sync::Mutex::new(std::collections::HashMap::new()),
            ),
            environment: None,
        }
    }

    /// Configure with a custom environment for testing
    ///
    /// Returns self for method chaining
    pub fn with_environment(mut self, env: Environment) -> Self {
        self.environment = Some(env);
        self
    }

    // Helper method to convert a relative path to an absolute path within the temp
    // directory
    fn temp_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.temp_dir.path().join(path)
        }
    }

    /// Configure with a mock output for a command
    ///
    /// Returns self for method chaining
    ///
    /// Note: This uses interior mutability, so it doesn't need &mut self
    pub async fn with_command_output(
        &self,
        command: String,
        stdout: String,
        stderr: String,
        exit_code: Option<i32>,
    ) -> &Self {
        let mut outputs = self.command_outputs.lock().await;
        outputs.insert(command.clone(), (command, stdout, stderr, exit_code));
        self
    }

    /// Configure with a mock exit status for a raw command
    ///
    /// Returns self for method chaining
    ///
    /// Note: This uses interior mutability, so it doesn't need &mut self
    pub async fn with_raw_command_output(
        &self,
        command: String,
        status: std::process::ExitStatus,
    ) -> &Self {
        let mut outputs = self.raw_command_outputs.lock().await;
        outputs.insert(command, status);
        self
    }

    /// Helper method to format a raw command with its args
    fn format_raw_command(command: &str, args: &[&str]) -> String {
        format!("{} {}", command, args.join(" "))
    }
}

impl Default for MockInfrastructure {
    fn default() -> Self {
        Self::new()
    }
}

impl Infrastructure for MockInfrastructure {
    type EnvironmentService = MockInfrastructure;
    type FsMetaService = MockInfrastructure;
    type FsReadService = MockInfrastructure;
    type FsRemoveService = MockInfrastructure;
    type FsSnapshotService = MockInfrastructure;
    type FsWriteService = MockInfrastructure;
    type FsUndoService = MockInfrastructure;
    type DirCreateService = MockInfrastructure;
    type CommandExecutorService = MockInfrastructure;
    type McpServer = MockInfrastructure;

    fn environment_service(&self) -> &Self::EnvironmentService {
        self
    }

    fn file_meta_service(&self) -> &Self::FsMetaService {
        self
    }

    fn file_read_service(&self) -> &Self::FsReadService {
        self
    }

    fn file_remove_service(&self) -> &Self::FsRemoveService {
        self
    }

    fn file_snapshot_service(&self) -> &Self::FsSnapshotService {
        self
    }

    fn file_write_service(&self) -> &Self::FsWriteService {
        self
    }

    fn file_undo_service(&self) -> &Self::FsUndoService {
        self
    }

    fn dir_create_service(&self) -> &Self::DirCreateService {
        self
    }

    fn command_executor_service(&self) -> &Self::CommandExecutorService {
        self
    }

    fn mcp_server(&self) -> &Self::McpServer {
        self
    }
}

impl EnvironmentService for MockInfrastructure {
    fn get_environment(&self) -> Environment {
        // Return stored environment if available, otherwise return a default one
        self.environment.clone().unwrap_or_else(|| {
            // Create a default environment for testing
            use std::path::PathBuf;

            // Use the factory method from Provider which handles URL parsing internally
            let provider = forge_domain::Provider::openai("mock-key");

            Environment {
                os: String::from("mock-os"),
                pid: 12345,
                cwd: self.temp_dir.path().to_path_buf(),
                home: Some(PathBuf::from("/mock/home")),
                shell: String::from("mock-shell"),
                base_path: self.temp_dir.path().to_path_buf(),
                provider,
                retry_config: forge_domain::RetryConfig::default(),
            }
        })
    }
}

#[async_trait]
impl FsMetaService for MockInfrastructure {
    async fn is_file(&self, path: &Path) -> anyhow::Result<bool> {
        let path = self.temp_path(path);
        Ok(ForgeFS::is_file(&path))
    }

    async fn exists(&self, path: &Path) -> anyhow::Result<bool> {
        let path = self.temp_path(path);
        Ok(ForgeFS::exists(&path))
    }
}

#[async_trait]
impl FsReadService for MockInfrastructure {
    async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
        let path = self.temp_path(path);
        ForgeFS::read_utf8(&path).await
    }

    async fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        let path = self.temp_path(path);
        ForgeFS::read(&path).await
    }

    async fn range_read_utf8(
        &self,
        path: &Path,
        start_char: u64,
        end_char: u64,
    ) -> anyhow::Result<(String, FileInfo)> {
        let path = self.temp_path(path);
        ForgeFS::read_range_utf8(&path, start_char, end_char).await
    }
}

#[async_trait]
impl FileRemoveService for MockInfrastructure {
    async fn remove(&self, path: &Path) -> anyhow::Result<()> {
        let path = self.temp_path(path);
        ForgeFS::remove_file(&path).await
    }
}

#[async_trait]
impl FsSnapshotService for MockInfrastructure {
    async fn create_snapshot(&self, file_path: &Path) -> Result<Snapshot> {
        // Simple implementation that creates a snapshot with minimal information
        let path = self.temp_path(file_path);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));

        Ok(Snapshot {
            id: SnapshotId::from(Uuid::new_v4()),
            timestamp,
            path: path.to_string_lossy().to_string(),
        })
    }

    async fn undo_snapshot(&self, _file_path: &Path) -> Result<()> {
        // In a real implementation, we'd restore from the last snapshot
        // For tests, we'll just indicate success
        Ok(())
    }
}

#[async_trait]
impl FsUndoService for MockInfrastructure {
    async fn undo(&self, _file_path: &Path) -> Result<()> {
        // In a real implementation, we'd restore from the last snapshot
        // For tests, we'll just indicate success
        Ok(())
    }
}

#[async_trait]
impl FsWriteService for MockInfrastructure {
    async fn write(&self, path: &Path, contents: Bytes) -> anyhow::Result<()> {
        let path = self.temp_path(path);
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            ForgeFS::create_dir_all(parent).await?;
        }
        ForgeFS::write(&path, contents).await
    }

    async fn write_temp(&self, prefix: &str, ext: &str, content: &str) -> anyhow::Result<PathBuf> {
        // Create a random filename in the temp directory
        let filename = format!("{}-{}{}", prefix, uuid::Uuid::new_v4(), ext);
        let path = self.temp_dir.path().join(filename);

        // Write content to the file
        let mut file = File::create(&path).await?;
        file.write_all(content.as_bytes()).await?;
        file.flush().await?;

        Ok(path)
    }
}

#[async_trait]
impl FsCreateDirsService for MockInfrastructure {
    async fn create_dirs(&self, path: &Path) -> anyhow::Result<()> {
        let path = self.temp_path(path);
        ForgeFS::create_dir_all(&path).await
    }
}

#[async_trait]
impl CommandExecutorService for MockInfrastructure {
    async fn execute_command(
        &self,
        command: String,
        _working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        // Check if we have a mock output for this command
        let outputs = self.command_outputs.lock().await;
        if let Some((cmd, stdout, stderr, exit_code)) = outputs.get(&command) {
            return Ok(CommandOutput {
                command: cmd.clone(),
                stdout: stdout.clone(),
                stderr: stderr.clone(),
                exit_code: *exit_code,
            });
        }

        // Default mock response if not specifically mocked
        Ok(CommandOutput {
            command,
            stdout: "".to_string(),
            stderr: "".to_string(),
            exit_code: Some(0),
        })
    }

    async fn execute_command_raw(
        &self,
        command: &str,
        args: &[&str],
    ) -> anyhow::Result<ExitStatus> {
        // Format the command string for lookup
        let cmd_key = Self::format_raw_command(command, args);

        // Check if we have a mock output for this raw command
        let outputs = self.raw_command_outputs.lock().await;
        if let Some(status) = outputs.get(&cmd_key) {
            return Ok(*status);
        }

        // Default mock exit status if not specifically mocked
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            Ok(ExitStatus::from_raw(0))
        }

        #[cfg(windows)]
        {
            // Windows default implementation
            anyhow::bail!("No mock exit status available for command: {}", cmd_key)
        }
    }
}

pub struct MockMcpClient;

#[async_trait]
impl McpClient for MockMcpClient {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        Ok(vec![])
    }

    async fn call(
        &self,
        _tool_name: &ToolName,
        _input: serde_json::Value,
    ) -> anyhow::Result<ToolOutput> {
        Ok(ToolOutput::default())
    }
}

#[async_trait]
impl McpServer for MockInfrastructure {
    type Client = MockMcpClient;

    async fn connect(&self, _config: McpServerConfig) -> anyhow::Result<Self::Client> {
        Ok(MockMcpClient)
    }
}
