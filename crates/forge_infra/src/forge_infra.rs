use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;

use bytes::Bytes;
use forge_domain::{CommandOutput, Conversation, ConversationId, Environment, McpServerConfig};
use forge_fs::FileInfo as FileInfoData;
use forge_services::{
    AppConfigRepository, CommandInfra, ConversationRepository, DirectoryReaderInfra,
    EnvironmentInfra, FileDirectoryInfra, FileInfoInfra, FileReaderInfra, FileRemoverInfra,
    FileWriterInfra, HttpInfra, McpServerInfra, SnapshotInfra, UserInfra, WalkerInfra,
};
use reqwest::header::HeaderMap;
use reqwest::{Response, Url};
use reqwest_eventsource::EventSource;

use crate::database::repository::app_config::AppConfigRepositoryImpl;
use crate::database::repository::conversation::ConversationRepositoryImpl;
use crate::database::{DatabasePool, PoolConfig};
use crate::env::ForgeEnvironmentInfra;
use crate::executor::ForgeCommandExecutorService;
use crate::fs_create_dirs::ForgeCreateDirsService;
use crate::fs_meta::ForgeFileMetaService;
use crate::fs_read::ForgeFileReadService;
use crate::fs_read_dir::ForgeDirectoryReaderService;
use crate::fs_remove::ForgeFileRemoveService;
use crate::fs_snap::ForgeFileSnapshotService;
use crate::fs_write::ForgeFileWriteService;
use crate::http::ForgeHttpInfra;
use crate::inquire::ForgeInquire;
use crate::mcp_client::ForgeMcpClient;
use crate::mcp_server::ForgeMcpServer;
use crate::walker::ForgeWalkerService;

#[derive(Clone)]
pub struct ForgeInfra {
    // TODO: Drop the "Service" suffix. Use names like ForgeFileReader, ForgeFileWriter,
    // ForgeHttpClient etc.
    file_read_service: Arc<ForgeFileReadService>,
    file_write_service: Arc<ForgeFileWriteService<ForgeFileSnapshotService>>,
    environment_service: Arc<ForgeEnvironmentInfra>,
    file_snapshot_service: Arc<ForgeFileSnapshotService>,
    file_meta_service: Arc<ForgeFileMetaService>,
    file_remove_service: Arc<ForgeFileRemoveService<ForgeFileSnapshotService>>,
    create_dirs_service: Arc<ForgeCreateDirsService>,
    directory_reader_service: Arc<ForgeDirectoryReaderService>,
    command_executor_service: Arc<ForgeCommandExecutorService>,
    inquire_service: Arc<ForgeInquire>,
    mcp_server: ForgeMcpServer,
    walker_service: Arc<ForgeWalkerService>,
    http_service: Arc<ForgeHttpInfra>,
    conversation_repository: Arc<ConversationRepositoryImpl>,
    app_config_repository: Arc<AppConfigRepositoryImpl>,
}

impl ForgeInfra {
    pub fn new(restricted: bool, cwd: PathBuf) -> Self {
        let environment_service = Arc::new(ForgeEnvironmentInfra::new(restricted, cwd));
        let env = environment_service.get_environment();
        let file_snapshot_service = Arc::new(ForgeFileSnapshotService::new(env.clone()));
        let http_service = Arc::new(ForgeHttpInfra::new(env.http.clone()));
        let db_pool =
            Arc::new(DatabasePool::try_from(PoolConfig::new(env.database_path())).unwrap());
        let conversation_repository =
            Arc::new(ConversationRepositoryImpl::new(db_pool, env.workspace_id()));

        let app_config_repository = Arc::new(AppConfigRepositoryImpl::new(
            env.app_config().as_path().to_path_buf(),
        ));

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
            directory_reader_service: Arc::new(ForgeDirectoryReaderService),
            command_executor_service: Arc::new(ForgeCommandExecutorService::new(
                restricted,
                env.clone(),
            )),
            inquire_service: Arc::new(ForgeInquire::new()),
            mcp_server: ForgeMcpServer,
            walker_service: Arc::new(ForgeWalkerService::new()),
            http_service,
            conversation_repository,
            app_config_repository,
        }
    }
}

impl EnvironmentInfra for ForgeInfra {
    fn get_environment(&self) -> Environment {
        self.environment_service.get_environment()
    }

    fn get_env_var(&self, key: &str) -> Option<String> {
        self.environment_service.get_env_var(key)
    }
}

#[async_trait::async_trait]
impl FileReaderInfra for ForgeInfra {
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
    ) -> anyhow::Result<(String, FileInfoData)> {
        self.file_read_service
            .range_read_utf8(path, start_line, end_line)
            .await
    }
}

#[async_trait::async_trait]
impl FileWriterInfra for ForgeInfra {
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
impl FileInfoInfra for ForgeInfra {
    async fn is_binary(&self, path: &Path) -> anyhow::Result<bool> {
        self.file_meta_service.is_binary(path).await
    }

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
impl SnapshotInfra for ForgeInfra {
    async fn create_snapshot(&self, file_path: &Path) -> anyhow::Result<forge_snaps::Snapshot> {
        self.file_snapshot_service.create_snapshot(file_path).await
    }

    async fn undo_snapshot(&self, file_path: &Path) -> anyhow::Result<()> {
        self.file_snapshot_service.undo_snapshot(file_path).await
    }
}

#[async_trait::async_trait]
impl FileRemoverInfra for ForgeInfra {
    async fn remove(&self, path: &Path) -> anyhow::Result<()> {
        self.file_remove_service.remove(path).await
    }
}

#[async_trait::async_trait]
impl FileDirectoryInfra for ForgeInfra {
    async fn create_dirs(&self, path: &Path) -> anyhow::Result<()> {
        self.create_dirs_service.create_dirs(path).await
    }
}

#[async_trait::async_trait]
impl CommandInfra for ForgeInfra {
    async fn execute_command(
        &self,
        command: String,
        working_dir: PathBuf,
        silent: bool,
        env_vars: Option<Vec<String>>,
    ) -> anyhow::Result<CommandOutput> {
        self.command_executor_service
            .execute_command(command, working_dir, silent, env_vars)
            .await
    }

    async fn execute_command_raw(
        &self,
        command: &str,
        working_dir: PathBuf,
        env_vars: Option<Vec<String>>,
    ) -> anyhow::Result<ExitStatus> {
        self.command_executor_service
            .execute_command_raw(command, working_dir, env_vars)
            .await
    }
}

#[async_trait::async_trait]
impl UserInfra for ForgeInfra {
    async fn prompt_question(&self, question: &str) -> anyhow::Result<Option<String>> {
        self.inquire_service.prompt_question(question).await
    }

    async fn select_one<T: std::fmt::Display + Send + 'static>(
        &self,
        message: &str,
        options: Vec<T>,
    ) -> anyhow::Result<Option<T>> {
        self.inquire_service.select_one(message, options).await
    }

    async fn select_many<T: std::fmt::Display + Clone + Send + 'static>(
        &self,
        message: &str,
        options: Vec<T>,
    ) -> anyhow::Result<Option<Vec<T>>> {
        self.inquire_service.select_many(message, options).await
    }
}

#[async_trait::async_trait]
impl McpServerInfra for ForgeInfra {
    type Client = ForgeMcpClient;

    async fn connect(&self, config: McpServerConfig) -> anyhow::Result<Self::Client> {
        self.mcp_server.connect(config).await
    }
}

#[async_trait::async_trait]
impl WalkerInfra for ForgeInfra {
    async fn walk(&self, config: forge_app::Walker) -> anyhow::Result<Vec<forge_app::WalkedFile>> {
        self.walker_service.walk(config).await
    }
}

#[async_trait::async_trait]
impl HttpInfra for ForgeInfra {
    async fn get(&self, url: &Url, headers: Option<HeaderMap>) -> anyhow::Result<Response> {
        self.http_service.get(url, headers).await
    }

    async fn post(&self, url: &Url, body: Bytes) -> anyhow::Result<Response> {
        self.http_service.post(url, body).await
    }

    async fn delete(&self, url: &Url) -> anyhow::Result<Response> {
        self.http_service.delete(url).await
    }
    async fn eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<EventSource> {
        self.http_service.eventsource(url, headers, body).await
    }
}
#[async_trait::async_trait]
impl DirectoryReaderInfra for ForgeInfra {
    async fn read_directory_files(
        &self,
        directory: &Path,
        pattern: Option<&str>,
    ) -> anyhow::Result<Vec<(PathBuf, String)>> {
        self.directory_reader_service
            .read_directory_files(directory, pattern)
            .await
    }
}

#[async_trait::async_trait]
impl ConversationRepository for ForgeInfra {
    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.conversation_repository
            .upsert_conversation(conversation)
            .await
    }

    async fn get_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>> {
        self.conversation_repository
            .get_conversation(conversation_id)
            .await
    }

    async fn get_all_conversations(
        &self,
        limit: Option<usize>,
    ) -> anyhow::Result<Option<Vec<Conversation>>> {
        self.conversation_repository
            .get_all_conversations(limit)
            .await
    }

    async fn get_last_conversation(&self) -> anyhow::Result<Option<Conversation>> {
        self.conversation_repository.get_last_conversation().await
    }
}
#[async_trait::async_trait]
impl AppConfigRepository for ForgeInfra {
    async fn get_app_config(&self) -> anyhow::Result<Option<forge_app::dto::AppConfig>> {
        self.app_config_repository.get_app_config().await
    }

    async fn set_app_config(&self, config: &forge_app::dto::AppConfig) -> anyhow::Result<()> {
        self.app_config_repository.set_app_config(config).await
    }
}
