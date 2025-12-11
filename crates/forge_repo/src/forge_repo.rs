use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use forge_app::{
    AgentRepository, CommandInfra, DirectoryReaderInfra, EnvironmentInfra, FileDirectoryInfra,
    FileInfoInfra, FileReaderInfra, FileRemoverInfra, FileWriterInfra, HttpInfra, KVStore,
    McpServerInfra, StrategyFactory, UserInfra, WalkedFile, Walker, WalkerInfra,
};
use forge_domain::{
    AnyProvider, AppConfig, AppConfigRepository, AuthCredential, CommandOutput, Conversation,
    ConversationId, ConversationRepository, Environment, FileInfo, McpServerConfig,
    MigrationResult, Provider, ProviderId, ProviderRepository, Skill, SkillRepository, Snapshot,
    SnapshotRepository,
};
// Re-export CacacheStorage from forge_infra
pub use forge_infra::CacacheStorage;
use reqwest::header::HeaderMap;
use reqwest::Response;
use reqwest_eventsource::EventSource;
use url::Url;

use crate::fs_snap::ForgeFileSnapshotService;
use crate::provider::ForgeProviderRepository;
use crate::{
    AppConfigRepositoryImpl, ConversationRepositoryImpl, DatabasePool, ForgeAgentRepository,
    ForgeSkillRepository, ForgeContextEngineRepository, ForgeValidationRepository, 
    ForgeWorkspaceRepository, PoolConfig,
};

/// Repository layer that implements all domain repository traits
///
/// This struct aggregates all repository implementations and provides a single
/// point of access for data persistence operations.
#[derive(Clone)]
pub struct ForgeRepo<F> {
    infra: Arc<F>,
    file_snapshot_service: Arc<ForgeFileSnapshotService>,
    conversation_repository: Arc<ConversationRepositoryImpl>,
    workspace_repository: Arc<ForgeWorkspaceRepository>,
    context_engine_repository: Arc<ForgeContextEngineRepository<F>>,
    validation_repository: Arc<ForgeValidationRepository<F>>,
    app_config_repository: Arc<AppConfigRepositoryImpl<F>>,
    mcp_cache_repository: Arc<CacacheStorage>,
    provider_repository: Arc<ForgeProviderRepository<F>>,
    agent_repository: Arc<ForgeAgentRepository<F>>,
    skill_repository: Arc<ForgeSkillRepository<F>>,
}

impl<F: forge_app::GrpcInfra + forge_app::EnvironmentInfra + forge_app::FileReaderInfra + forge_app::FileWriterInfra> ForgeRepo<F> {
    pub fn new(infra: Arc<F>) -> Self {
        let env = infra.get_environment();
        let file_snapshot_service = Arc::new(ForgeFileSnapshotService::new(env.clone()));
        let db_pool =
            Arc::new(DatabasePool::try_from(PoolConfig::new(env.database_path())).unwrap());
        let conversation_repository =
            Arc::new(ConversationRepositoryImpl::new(db_pool, env.project_root_id()));

        let app_config_repository = Arc::new(AppConfigRepositoryImpl::new(infra.clone()));

        let mcp_cache_repository = Arc::new(CacacheStorage::new(
            env.cache_dir().join("mcp_cache"),
            Some(3600),
        )); // 1 hour TTL

        let workspace_repository = Arc::new(ForgeWorkspaceRepository::new(
            Arc::new(DatabasePool::try_from(PoolConfig::new(env.database_path())).unwrap()),
        ));
        let context_engine_repository = Arc::new(ForgeContextEngineRepository::new(
            infra.clone(),
        ));
        let validation_repository = Arc::new(ForgeValidationRepository::new(infra.clone()));

        let provider_repository = Arc::new(ForgeProviderRepository::new(infra.clone()));
        let agent_repository = Arc::new(ForgeAgentRepository::new(infra.clone()));
        let skill_repository = Arc::new(ForgeSkillRepository::new(infra.clone()));
        Self {
            infra,
            file_snapshot_service,
            conversation_repository,
            workspace_repository,
            context_engine_repository,
            validation_repository,
            app_config_repository,
            mcp_cache_repository,
            provider_repository,
            agent_repository,
            skill_repository,
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

    async fn delete_conversation(&self, conversation_id: &ConversationId) -> anyhow::Result<()> {
        self.conversation_repository
            .delete_conversation(conversation_id)
            .await
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + FileReaderInfra + FileWriterInfra + Send + Sync> ProviderRepository
    for ForgeRepo<F>
{
    async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>> {
        self.provider_repository.get_all_providers().await
    }

    async fn get_provider(&self, id: ProviderId) -> anyhow::Result<Provider<Url>> {
        self.provider_repository.get_provider(id).await
    }

    async fn upsert_credential(&self, credential: AuthCredential) -> anyhow::Result<()> {
        self.provider_repository.upsert_credential(credential).await
    }

    async fn get_credential(&self, id: &ProviderId) -> anyhow::Result<Option<AuthCredential>> {
        self.provider_repository.get_credential(id).await
    }

    async fn remove_credential(&self, id: &ProviderId) -> anyhow::Result<()> {
        self.provider_repository.remove_credential(id).await
    }

    async fn migrate_env_credentials(&self) -> anyhow::Result<Option<MigrationResult>> {
        self.provider_repository.migrate_env_to_file().await
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

    fn get_env_vars(&self) -> BTreeMap<String, String> {
        self.infra.get_env_vars()
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
    async fn list_directory_entries(
        &self,
        directory: &Path,
    ) -> anyhow::Result<Vec<(PathBuf, bool)>> {
        self.infra.list_directory_entries(directory).await
    }

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

    async fn connect(
        &self,
        config: McpServerConfig,
        env_vars: &BTreeMap<String, String>,
    ) -> anyhow::Result<F::Client> {
        self.infra.connect(config, env_vars).await
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

#[async_trait::async_trait]
impl<F: FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra + Send + Sync> AgentRepository
    for ForgeRepo<F>
{
    async fn get_agents(&self) -> anyhow::Result<Vec<forge_domain::AgentDefinition>> {
        self.agent_repository.get_agents().await
    }
}

#[async_trait::async_trait]
impl<F: FileInfoInfra + EnvironmentInfra + FileReaderInfra + WalkerInfra + Send + Sync>
    SkillRepository for ForgeRepo<F>
{
    async fn load_skills(&self) -> anyhow::Result<Vec<Skill>> {
        self.skill_repository.load_skills().await
    }
}

impl<F: StrategyFactory> StrategyFactory for ForgeRepo<F> {
    type Strategy = F::Strategy;

    fn create_auth_strategy(
        &self,
        provider_id: ProviderId,
        auth_method: forge_domain::AuthMethod,
        required_params: Vec<forge_domain::URLParam>,
    ) -> anyhow::Result<Self::Strategy> {
        self.infra
            .create_auth_strategy(provider_id, auth_method, required_params)
    }
}

// Implement the required repository traits for ForgeRepo
#[async_trait::async_trait]
impl<F> forge_domain::WorkspaceRepository for ForgeRepo<F>
where
    F: Send + Sync,
{
    async fn upsert(
        &self,
        workspace_id: &forge_domain::WorkspaceId,
        user_id: &forge_domain::UserId,
        path: &std::path::Path,
    ) -> anyhow::Result<()> {
        self.workspace_repository.upsert(workspace_id, user_id, path).await
    }

    async fn find_by_path(&self, path: &std::path::Path) -> anyhow::Result<Option<forge_domain::Workspace>> {
        self.workspace_repository.find_by_path(path).await
    }

    async fn get_user_id(&self) -> anyhow::Result<Option<forge_domain::UserId>> {
        self.workspace_repository.get_user_id().await
    }

    async fn delete(&self, workspace_id: &forge_domain::WorkspaceId) -> anyhow::Result<()> {
        self.workspace_repository.delete(workspace_id).await
    }
}

#[async_trait::async_trait]
impl<F> forge_domain::ContextEngineRepository for ForgeRepo<F>
where
    F: forge_app::GrpcInfra + Send + Sync,
{
    async fn authenticate(&self) -> anyhow::Result<forge_domain::WorkspaceAuth> {
        self.context_engine_repository.authenticate().await
    }

    async fn create_workspace(
        &self,
        working_dir: &std::path::Path,
        auth_token: &forge_domain::ApiKey,
    ) -> anyhow::Result<forge_domain::WorkspaceId> {
        self.context_engine_repository.create_workspace(working_dir, auth_token).await
    }

    async fn upload_files(
        &self,
        upload: &forge_domain::FileUpload,
        auth_token: &forge_domain::ApiKey,
    ) -> anyhow::Result<forge_domain::FileUploadInfo> {
        self.context_engine_repository.upload_files(upload, auth_token).await
    }

    async fn search(
        &self,
        query: &forge_domain::CodeSearchQuery<'_>,
        auth_token: &forge_domain::ApiKey,
    ) -> anyhow::Result<Vec<forge_domain::Node>> {
        self.context_engine_repository.search(query, auth_token).await
    }

    async fn list_workspaces(
        &self,
        auth_token: &forge_domain::ApiKey,
    ) -> anyhow::Result<Vec<forge_domain::WorkspaceInfo>> {
        self.context_engine_repository.list_workspaces(auth_token).await
    }

    async fn get_workspace(
        &self,
        workspace_id: &forge_domain::WorkspaceId,
        auth_token: &forge_domain::ApiKey,
    ) -> anyhow::Result<Option<forge_domain::WorkspaceInfo>> {
        self.context_engine_repository.get_workspace(workspace_id, auth_token).await
    }

    async fn list_workspace_files(
        &self,
        workspace: &forge_domain::WorkspaceFiles,
        auth_token: &forge_domain::ApiKey,
    ) -> anyhow::Result<Vec<forge_domain::FileHash>> {
        self.context_engine_repository.list_workspace_files(workspace, auth_token).await
    }

    async fn delete_files(
        &self,
        deletion: &forge_domain::FileDeletion,
        auth_token: &forge_domain::ApiKey,
    ) -> anyhow::Result<()> {
        self.context_engine_repository.delete_files(deletion, auth_token).await
    }

    async fn delete_workspace(
        &self,
        workspace_id: &forge_domain::WorkspaceId,
        auth_token: &forge_domain::ApiKey,
    ) -> anyhow::Result<()> {
        self.context_engine_repository.delete_workspace(workspace_id, auth_token).await
    }
}

#[async_trait::async_trait]
impl<F> forge_domain::ValidationRepository for ForgeRepo<F>
where
    F: forge_app::GrpcInfra + Send + Sync,
{
    async fn validate_file(
        &self,
        path: impl AsRef<std::path::Path> + Send,
        content: &str,
    ) -> anyhow::Result<Option<String>> {
        self.validation_repository.validate_file(path, content).await
    }
}
impl<F: forge_app::GrpcInfra> forge_app::GrpcInfra for ForgeRepo<F> {
    fn channel(&self) -> tonic::transport::Channel {
        self.infra.channel()
    }

    fn hydrate(&self) {
        self.infra.hydrate()
    }
}