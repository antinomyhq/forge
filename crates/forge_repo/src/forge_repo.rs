use std::path::Path;
use std::sync::Arc;

use forge_app::AppConfigRepository;
use forge_domain::{
    CacheRepository, Conversation, ConversationId, ConversationRepository, ModelId, Provider,
    ProviderId, ProviderRepository, Snapshot, SnapshotRepository,
};

use crate::fs_snap::ForgeFileSnapshotService;
use crate::{
    AppConfigRepositoryImpl, CacacheRepository, ConversationRepositoryImpl, DatabasePool,
    PoolConfig,
};

/// Repository layer that implements all domain repository traits
///
/// This struct aggregates all repository implementations and provides a single
/// point of access for data persistence operations.
#[derive(Clone)]
pub struct ForgeRepo {
    file_snapshot_service: Arc<ForgeFileSnapshotService>,
    conversation_repository: Arc<ConversationRepositoryImpl>,
    app_config_repository: Arc<AppConfigRepositoryImpl>,
    mcp_cache_repository: Arc<CacacheRepository>,
}

impl ForgeRepo {
    pub fn new(env: forge_app::domain::Environment) -> Self {
        let file_snapshot_service = Arc::new(ForgeFileSnapshotService::new(env.clone()));
        let db_pool =
            Arc::new(DatabasePool::try_from(PoolConfig::new(env.database_path())).unwrap());
        let conversation_repository =
            Arc::new(ConversationRepositoryImpl::new(db_pool, env.workspace_id()));

        let app_config_repository = Arc::new(AppConfigRepositoryImpl::new(
            env.app_config().as_path().to_path_buf(),
        ));

        let mcp_cache_repository = Arc::new(CacacheRepository::new(
            env.cache_dir().join("mcp_cache"),
            Some(3600),
        )); // 1 hour TTL
        Self {
            file_snapshot_service,
            conversation_repository,
            app_config_repository,
            mcp_cache_repository,
        }
    }
}

#[async_trait::async_trait]
impl SnapshotRepository for ForgeRepo {
    async fn insert_snapshot(&self, file_path: &Path) -> anyhow::Result<Snapshot> {
        self.file_snapshot_service.insert_snapshot(file_path).await
    }

    async fn undo_snapshot(&self, file_path: &Path) -> anyhow::Result<()> {
        self.file_snapshot_service.undo_snapshot(file_path).await
    }
}

#[async_trait::async_trait]
impl ConversationRepository for ForgeRepo {
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
impl ProviderRepository for ForgeRepo {
    async fn get_default_provider(&self) -> anyhow::Result<Provider> {
        unimplemented!()
    }

    async fn set_default_provider(&self, _provider_id: ProviderId) -> anyhow::Result<()> {
        unimplemented!()
    }

    async fn get_all_providers(&self) -> anyhow::Result<Vec<Provider>> {
        unimplemented!()
    }

    async fn get_default_model(&self, _provider_id: &ProviderId) -> anyhow::Result<ModelId> {
        unimplemented!()
    }

    async fn set_default_model(
        &self,
        _model: ModelId,
        _provider_id: ProviderId,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }

    async fn get_provider(&self, _id: ProviderId) -> anyhow::Result<Provider> {
        unimplemented!()
    }
}

#[async_trait::async_trait]
impl AppConfigRepository for ForgeRepo {
    async fn get_app_config(&self) -> anyhow::Result<forge_app::dto::AppConfig> {
        self.app_config_repository.get_app_config().await
    }

    async fn set_app_config(&self, config: &forge_app::dto::AppConfig) -> anyhow::Result<()> {
        self.app_config_repository.set_app_config(config).await
    }
}

#[async_trait::async_trait]
impl CacheRepository for ForgeRepo {
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
