use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use forge_app::{
    CommandInfra, DirectoryReaderInfra, EnvironmentInfra, FileDirectoryInfra, FileInfoInfra,
    FileReaderInfra, FileRemoverInfra, FileWriterInfra, HttpInfra, KVStore, McpServerInfra,
    UserInfra, WalkedFile, Walker, WalkerInfra,
};
use forge_domain::{
    AppConfig, AppConfigRepository, CommandOutput, Conversation, ConversationId,
    ConversationRepository, Environment, FileInfo, McpServerConfig, Provider, ProviderId,
    ProviderRepository, Snapshot, SnapshotRepository,
};
use forge_infra::CacacheStorage;
use reqwest::header::HeaderMap;
use reqwest::Response;
use reqwest_eventsource::EventSource;
use url::Url;

use crate::fs_snap::ForgeFileSnapshotService;
use crate::provider::ForgeProviderRepository;
use crate::{AppConfigRepositoryImpl, ConversationRepositoryImpl, DatabasePool, PoolConfig};

/// Repository layer that implements all domain repository traits
///
/// This struct aggregates all repository implementations and provides a single
/// point of access for data persistence operations.
#[derive(Clone)]
pub struct ForgeRepo<F> {
    infra: Arc<F>,
    file_snapshot_service: Arc<ForgeFileSnapshotService>,
    conversation_repository: Arc<ConversationRepositoryImpl>,
    app_config_repository: Arc<AppConfigRepositoryImpl<F>>,
    mcp_cache_repository: Arc<CacacheStorage>,
    provider_repository: Arc<ForgeProviderRepository<F>>,
}

impl<F: EnvironmentInfra + FileReaderInfra + FileWriterInfra> ForgeRepo<F> {
    pub fn new(infra: Arc<F>) -> Self {
        let env = infra.get_environment();
        let file_snapshot_service = Arc::new(ForgeFileSnapshotService::new(env.clone()));
        let db_pool =
            Arc::new(DatabasePool::try_from(PoolConfig::new(env.database_path())).unwrap());
        let conversation_repository =
            Arc::new(ConversationRepositoryImpl::new(db_pool, env.workspace_id()));

        let app_config_repository = Arc::new(AppConfigRepositoryImpl::new(infra.clone()));

        let mcp_cache_repository = Arc::new(CacacheStorage::new(
            env.cache_dir().join("mcp_cache"),
            Some(3600),
        )); // 1 hour TTL

        let provider_repository = Arc::new(ForgeProviderRepository::new(infra.clone()));
        Self {
            infra,
            file_snapshot_service,
            conversation_repository,
            app_config_repository,
            mcp_cache_repository,
            provider_repository,
        }
    }
}

#[async_trait::async_trait]
impl<F: Send + Sync> SnapshotRepository for ForgeRepo<F> {
    async fn insert_snapshot(&self, file_path: &Path) -> anyhow::Result<Snapshot> {
        self.file_snapshot_service.insert_snapshot(file_path).await
    }

    async fn undo_snapshot(&self, file_path: &Path) -> anyhow::Result<()> {
        self.file_snapshot_service.undo_snapshot(file_path).await
    }
}

#[async_trait::async_trait]
impl<F: Send + Sync> ConversationRepository for ForgeRepo<F> {
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
impl<F: EnvironmentInfra + FileReaderInfra + Send + Sync> ProviderRepository for ForgeRepo<F> {
    async fn get_all_providers(&self) -> anyhow::Result<Vec<Provider>> {
        self.provider_repository.get_all_providers().await
    }

    async fn get_provider(&self, id: ProviderId) -> anyhow::Result<Provider> {
        self.provider_repository.get_provider(id).await
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + FileReaderInfra + FileWriterInfra + Send + Sync> AppConfigRepository
    for ForgeRepo<F>
{
    async fn get_app_config(&self) -> anyhow::Result<AppConfig> {
        self.app_config_repository.get_app_config().await
    }

    async fn set_app_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        self.app_config_repository.set_app_config(config).await
    }
}

#[async_trait::async_trait]
impl<F: Send + Sync> KVStore for ForgeRepo<F> {
    async fn cache_get<K, V>(&self, key: &K) -> anyhow::Result<Option<V>>
    where
        K: std::hash::Hash + Sync,
        V: serde::Serialize + serde::de::DeserializeOwned + Send,
    {
        self.mcp_cache_repository.cache_get(key).await
    }

    async fn cache_set<K, V>(&self, key: &K, value: &V) -> anyhow::Result<()>
    where
        K: std::hash::Hash + Sync,
        V: serde::Serialize + Sync,
    {
        self.mcp_cache_repository.cache_set(key, value).await
    }

    async fn cache_clear(&self) -> anyhow::Result<()> {
        self.mcp_cache_repository.cache_clear().await
    }
}

#[async_trait::async_trait]
impl<F: HttpInfra> HttpInfra for ForgeRepo<F> {
    async fn get(&self, url: &Url, headers: Option<HeaderMap>) -> anyhow::Result<Response> {
        self.infra.get(url, headers).await
    }

    async fn post(&self, url: &Url, body: Bytes) -> anyhow::Result<Response> {
        self.infra.post(url, body).await
    }

    async fn delete(&self, url: &Url) -> anyhow::Result<Response> {
        self.infra.delete(url).await
    }

    async fn eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<EventSource> {
        self.infra.eventsource(url, headers, body).await
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra> EnvironmentInfra for ForgeRepo<F> {
    fn get_environment(&self) -> Environment {
        self.infra.get_environment()
    }
    fn get_env_var(&self, key: &str) -> Option<String> {
        self.infra.get_env_var(key)
    }
}

#[async_trait::async_trait]
impl<F> FileReaderInfra for ForgeRepo<F>
where
    F: FileReaderInfra + Send + Sync,
{
    async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
        self.infra.read_utf8(path).await
    }
    async fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        self.infra.read(path).await
    }

    async fn range_read_utf8(
        &self,
        path: &Path,
        start_line: u64,
        end_line: u64,
    ) -> anyhow::Result<(String, FileInfo)> {
        self.infra.range_read_utf8(path, start_line, end_line).await
    }
}

#[async_trait::async_trait]
impl<F> WalkerInfra for ForgeRepo<F>
where
    F: WalkerInfra + Send + Sync,
{
    async fn walk(&self, config: Walker) -> anyhow::Result<Vec<WalkedFile>> {
        self.infra.walk(config).await
    }
}

#[async_trait::async_trait]
impl<F> FileWriterInfra for ForgeRepo<F>
where
    F: FileWriterInfra + Send + Sync,
{
    async fn write(&self, path: &Path, contents: Bytes) -> anyhow::Result<()> {
        self.infra.write(path, contents).await
    }
    async fn write_temp(&self, prefix: &str, ext: &str, content: &str) -> anyhow::Result<PathBuf> {
        self.infra.write_temp(prefix, ext, content).await
    }
}

#[async_trait::async_trait]
impl<F> FileInfoInfra for ForgeRepo<F>
where
    F: FileInfoInfra + Send + Sync,
{
    async fn is_binary(&self, path: &Path) -> anyhow::Result<bool> {
        self.infra.is_binary(path).await
    }
    async fn is_file(&self, path: &Path) -> anyhow::Result<bool> {
        self.infra.is_file(path).await
    }
    async fn exists(&self, path: &Path) -> anyhow::Result<bool> {
        self.infra.exists(path).await
    }
    async fn file_size(&self, path: &Path) -> anyhow::Result<u64> {
        self.infra.file_size(path).await
    }
}

#[async_trait::async_trait]
impl<F> FileDirectoryInfra for ForgeRepo<F>
where
    F: FileDirectoryInfra + Send + Sync,
{
    async fn create_dirs(&self, path: &Path) -> anyhow::Result<()> {
        self.infra.create_dirs(path).await
    }
}

#[async_trait::async_trait]
impl<F> FileRemoverInfra for ForgeRepo<F>
where
    F: FileRemoverInfra + Send + Sync,
{
    async fn remove(&self, path: &Path) -> anyhow::Result<()> {
        self.infra.remove(path).await
    }
}

#[async_trait::async_trait]
impl<F> DirectoryReaderInfra for ForgeRepo<F>
where
    F: DirectoryReaderInfra + Send + Sync,
{
    async fn read_directory_files(
        &self,
        directory: &Path,
        pattern: Option<&str>, // Optional glob pattern like "*.md"
    ) -> anyhow::Result<Vec<(PathBuf, String)>> {
        self.infra.read_directory_files(directory, pattern).await
    }
}

#[async_trait::async_trait]
impl<F> UserInfra for ForgeRepo<F>
where
    F: UserInfra + Send + Sync,
{
    async fn prompt_question(&self, question: &str) -> anyhow::Result<Option<String>> {
        self.infra.prompt_question(question).await
    }

    async fn select_one<T: std::fmt::Display + Send + 'static>(
        &self,
        message: &str,
        options: Vec<T>,
    ) -> anyhow::Result<Option<T>> {
        self.infra.select_one(message, options).await
    }

    async fn select_one_enum<T>(&self, message: &str) -> anyhow::Result<Option<T>>
    where
        T: std::fmt::Display + Send + 'static + strum::IntoEnumIterator + std::str::FromStr,
        <T as std::str::FromStr>::Err: std::fmt::Debug,
    {
        self.infra.select_one_enum(message).await
    }

    async fn select_many<T: std::fmt::Display + Clone + Send + 'static>(
        &self,
        message: &str,
        options: Vec<T>,
    ) -> anyhow::Result<Option<Vec<T>>> {
        self.infra.select_many(message, options).await
    }
}

#[async_trait::async_trait]
impl<F> McpServerInfra for ForgeRepo<F>
where
    F: McpServerInfra + Send + Sync,
{
    type Client = F::Client;

    async fn connect(&self, config: McpServerConfig) -> anyhow::Result<F::Client> {
        self.infra.connect(config).await
    }
}

#[async_trait::async_trait]
impl<F> CommandInfra for ForgeRepo<F>
where
    F: CommandInfra + Send + Sync,
{
    async fn execute_command(
        &self,
        command: String,
        working_dir: PathBuf,
        silent: bool,
        env_vars: Option<Vec<String>>,
    ) -> anyhow::Result<CommandOutput> {
        self.infra
            .execute_command(command, working_dir, silent, env_vars)
            .await
    }

    async fn execute_command_raw(
        &self,
        command: &str,
        working_dir: PathBuf,
        env_vars: Option<Vec<String>>,
    ) -> anyhow::Result<std::process::ExitStatus> {
        self.infra
            .execute_command_raw(command, working_dir, env_vars)
            .await
    }
}
