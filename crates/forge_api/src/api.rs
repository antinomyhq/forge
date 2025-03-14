use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use forge_app::{EnvironmentService, ForgeApp, FsSnapshotService, Infrastructure};
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_snaps::Snapshot;
use forge_stream::MpscStream;
use serde_json::Value;

use crate::executor::ForgeExecutorService;
use crate::loader::ForgeLoaderService;
use crate::suggestion::ForgeSuggestionService;
use crate::API;

pub struct ForgeAPI<F> {
    app: Arc<F>,
    executor_service: ForgeExecutorService<F>,
    suggestion_service: ForgeSuggestionService<F>,
    loader: ForgeLoaderService<F>,
}

impl<F: App + Infrastructure> ForgeAPI<F> {
    pub fn new(app: Arc<F>) -> Self {
        Self {
            app: app.clone(),
            executor_service: ForgeExecutorService::new(app.clone()),
            suggestion_service: ForgeSuggestionService::new(app.clone()),
            loader: ForgeLoaderService::new(app.clone()),
        }
    }
}

impl ForgeAPI<ForgeApp<ForgeInfra>> {
    pub fn init(restricted: bool) -> Self {
        let infra = Arc::new(ForgeInfra::new(restricted));
        let app = Arc::new(ForgeApp::new(infra));
        ForgeAPI::new(app)
    }
}

#[async_trait::async_trait]
impl<F: App + Infrastructure> API for ForgeAPI<F> {
    async fn list_snapshots(&self, path: Option<&Path>) -> Result<Vec<Snapshot>> {
        self.app.file_snapshot_service().list_snapshots(path).await
    }

    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: u128) -> anyhow::Result<()> {
        self.app
            .file_snapshot_service()
            .restore_by_timestamp(file_path, timestamp)
            .await
    }

    async fn restore_by_hash(&self, file_path: &Path, hash: &str) -> anyhow::Result<()> {
        self.app
            .file_snapshot_service()
            .restore_by_hash(file_path, hash)
            .await
    }

    async fn restore_previous(&self, file_path: &Path) -> Result<()> {
        self.app
            .file_snapshot_service()
            .restore_previous(file_path)
            .await
    }

    async fn get_snapshot_by_timestamp(
        &self,
        file_path: &Path,
        timestamp: u128,
    ) -> anyhow::Result<forge_snaps::Snapshot> {
        self.app
            .file_snapshot_service()
            .get_snapshot_by_timestamp(file_path, timestamp)
            .await
    }

    async fn get_snapshot_by_hash(
        &self,
        file_path: &Path,
        hash: &str,
    ) -> anyhow::Result<forge_snaps::Snapshot> {
        self.app
            .file_snapshot_service()
            .get_snapshot_by_hash(file_path, hash)
            .await
    }

    async fn purge_older_than(&self, days: u32) -> Result<usize> {
        self.app
            .file_snapshot_service()
            .purge_older_than(days)
            .await
    }

    async fn suggestions(&self) -> Result<Vec<File>> {
        self.suggestion_service.suggestions().await
    }

    async fn tools(&self) -> Vec<ToolDefinition> {
        self.app.tool_service().list()
    }

    async fn models(&self) -> Result<Vec<Model>> {
        Ok(self.app.provider_service().models().await?)
    }

    async fn chat(
        &self,
        chat: ChatRequest,
    ) -> anyhow::Result<MpscStream<Result<AgentMessage<ChatResponse>, anyhow::Error>>> {
        Ok(self.executor_service.chat(chat).await?)
    }

    async fn init(&self, workflow: Workflow) -> anyhow::Result<ConversationId> {
        self.app.conversation_service().create(workflow).await
    }

    fn environment(&self) -> Environment {
        self.app.environment_service().get_environment().clone()
    }

    async fn load(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        self.loader.load(path).await
    }

    async fn conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>> {
        self.app.conversation_service().get(conversation_id).await
    }

    async fn get_variable(
        &self,
        conversation_id: &ConversationId,
        key: &str,
    ) -> anyhow::Result<Option<Value>> {
        self.app
            .conversation_service()
            .get_variable(conversation_id, key)
            .await
    }

    async fn set_variable(
        &self,
        conversation_id: &ConversationId,
        key: String,
        value: Value,
    ) -> anyhow::Result<()> {
        self.app
            .conversation_service()
            .set_variable(conversation_id, key, value)
            .await
    }

    async fn get_latest_snapshot(&self, file_path: &Path) -> Result<Snapshot> {
        self.app.file_snapshot_service().get_latest(file_path).await
    }
}
