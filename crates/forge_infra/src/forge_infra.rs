use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;

use bytes::Bytes;
use forge_app::EnvironmentService;
use forge_domain::{CommandOutput, Environment, McpServerConfig};
use forge_fs::FileInfo;
use forge_services::{
    CommandExecutorService, FileRemoveService, FsCreateDirsService, FsMetaService, FsReadService,
    FsSnapshotService, FsWriteService, InquireService, McpServer,
};

use crate::env::ForgeEnvironmentService;
use crate::executor::ForgeCommandExecutorService;
use crate::fs_create_dirs::ForgeCreateDirsService;
use crate::fs_meta::ForgeFileMetaService;
use crate::fs_read::ForgeFileReadService;
use crate::fs_remove::ForgeFileRemoveService;
use crate::fs_snap::ForgeFileSnapshotService;
use crate::fs_write::ForgeFileWriteService;
use crate::inquire::ForgeInquire;
use crate::mcp_client::ForgeMcpClient;
use crate::mcp_server::ForgeMcpServer;

#[derive(Clone)]
pub struct ForgeInfra {
    file_read_service: Arc<ForgeFileReadService>,
    file_write_service: Arc<ForgeFileWriteService<ForgeFileSnapshotService>>,
    environment_service: Arc<ForgeEnvironmentService>,
    file_snapshot_service: Arc<ForgeFileSnapshotService>,
    file_meta_service: Arc<ForgeFileMetaService>,
    file_remove_service: Arc<ForgeFileRemoveService<ForgeFileSnapshotService>>,
    create_dirs_service: Arc<ForgeCreateDirsService>,
    command_executor_service: Arc<ForgeCommandExecutorService>,
    inquire_service: Arc<ForgeInquire>,
    mcp_server: ForgeMcpServer,
}

impl ForgeInfra {
    pub fn new(restricted: bool) -> Self {
        let environment_service = Arc::new(ForgeEnvironmentService::new(restricted));
        let env = environment_service.get_environment();
        let file_snapshot_service = Arc::new(ForgeFileSnapshotService::new(env.clone()));
        Self {
            file_read_service: Arc::new(ForgeFileReadService::new()),
            file_write_service: Arc::new(ForgeFileWriteService::new(file_snapshot_service.clone())),
            file_meta_service: Arc::new(ForgeFileMetaService),
            file_remove_service: Arc::new(ForgeFileRemoveService::new(
                file_snapshot_service.clone(),
            )),
            environment_service,
            file_snapshot_service,
            create_dirs_service: Arc::new(ForgeCreateDirsService),
            command_executor_service: Arc::new(ForgeCommandExecutorService::new(
                restricted,
                env.clone(),
            )),
            inquire_service: Arc::new(ForgeInquire::new()),
            mcp_server: ForgeMcpServer,
        }
    }
}

impl EnvironmentService for ForgeInfra {
    fn get_environment(&self) -> Environment {
        self.environment_service.get_environment()
    }
}

#[async_trait::async_trait]
impl FsReadService for ForgeInfra {
    async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
        self.file_read_service.read_utf8(path).await
    }

    async fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        self.file_read_service.read(path).await
    }

    async fn range_read_utf8(
        &self,
        path: &Path,
        start_line: u64,
        end_line: u64,
    ) -> anyhow::Result<(String, FileInfo)> {
        self.file_read_service
            .range_read_utf8(path, start_line, end_line)
            .await
    }
}

#[async_trait::async_trait]
impl FsWriteService for ForgeInfra {
    async fn write(
        &self,
        path: &Path,
        contents: Bytes,
        capture_snapshot: bool,
    ) -> anyhow::Result<()> {
        self.file_write_service
            .write(path, contents, capture_snapshot)
            .await
    }

    async fn write_temp(&self, prefix: &str, ext: &str, content: &str) -> anyhow::Result<PathBuf> {
        self.file_write_service
            .write_temp(prefix, ext, content)
            .await
    }
}

#[async_trait::async_trait]
impl FsMetaService for ForgeInfra {
    async fn is_file(&self, path: &Path) -> anyhow::Result<bool> {
        self.file_meta_service.is_file(path).await
    }

    async fn exists(&self, path: &Path) -> anyhow::Result<bool> {
        self.file_meta_service.exists(path).await
    }

    async fn file_size(&self, path: &Path) -> anyhow::Result<u64> {
        self.file_meta_service.file_size(path).await
    }
}

#[async_trait::async_trait]
impl FsSnapshotService for ForgeInfra {
    async fn create_snapshot(&self, file_path: &Path) -> anyhow::Result<forge_snaps::Snapshot> {
        self.file_snapshot_service.create_snapshot(file_path).await
    }

    async fn undo_snapshot(&self, file_path: &Path) -> anyhow::Result<()> {
        self.file_snapshot_service.undo_snapshot(file_path).await
    }
}

#[async_trait::async_trait]
impl FileRemoveService for ForgeInfra {
    async fn remove(&self, path: &Path) -> anyhow::Result<()> {
        self.file_remove_service.remove(path).await
    }
}

#[async_trait::async_trait]
impl FsCreateDirsService for ForgeInfra {
    async fn create_dirs(&self, path: &Path) -> anyhow::Result<()> {
        self.create_dirs_service.create_dirs(path).await
    }
}

#[async_trait::async_trait]
impl CommandExecutorService for ForgeInfra {
    async fn execute_command(
        &self,
        command: String,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        self.command_executor_service
            .execute_command(command, working_dir)
            .await
    }

    async fn execute_command_raw(&self, command: &str) -> anyhow::Result<ExitStatus> {
        self.command_executor_service
            .execute_command_raw(command)
            .await
    }
}

#[async_trait::async_trait]
impl InquireService for ForgeInfra {
    async fn prompt_question(&self, question: &str) -> anyhow::Result<Option<String>> {
        self.inquire_service.prompt_question(question).await
    }

    async fn select_one(
        &self,
        message: &str,
        options: Vec<String>,
    ) -> anyhow::Result<Option<String>> {
        self.inquire_service.select_one(message, options).await
    }

    async fn select_many(
        &self,
        message: &str,
        options: Vec<String>,
    ) -> anyhow::Result<Option<Vec<String>>> {
        self.inquire_service.select_many(message, options).await
    }
}

#[async_trait::async_trait]
impl McpServer for ForgeInfra {
    type Client = ForgeMcpClient;

    async fn connect(&self, config: McpServerConfig) -> anyhow::Result<Self::Client> {
        self.mcp_server.connect(config).await
    }
}
