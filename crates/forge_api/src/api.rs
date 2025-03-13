use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use forge_app::{EnvironmentService, ForgeApp, FsSnapshotService, Infrastructure};
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_snaps::{SnapshotInfo, SnapshotMetadata};
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
    async fn list_snapshots(&self, file_path: &Path) -> Result<Vec<SnapshotInfo>> {
        self.app
            .file_snapshot_service()
            .list_snapshots(file_path)
            .await
    }

    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: &str) -> Result<()> {
        self.app
            .file_snapshot_service()
            .restore_by_timestamp(file_path, timestamp)
            .await
    }

    async fn restore_by_index(&self, file_path: &Path, index: isize) -> Result<()> {
        self.app
            .file_snapshot_service()
            .restore_by_index(file_path, index)
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
        timestamp: &str,
    ) -> Result<SnapshotMetadata> {
        self.app
            .file_snapshot_service()
            .get_snapshot_by_timestamp(file_path, timestamp)
            .await
    }

    async fn get_snapshot_by_index(
        &self,
        file_path: &Path,
        index: isize,
    ) -> Result<SnapshotMetadata> {
        self.app
            .file_snapshot_service()
            .get_snapshot_by_index(file_path, index)
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

    async fn dispatch(&self, event_name: &str, event_value: &str) -> anyhow::Result<()> {
        // Create an event with the specified name and value
        let event = Event::new(event_name, event_value);

        // Create a new conversation with default workflow if we don't have one
        let dummy_workflow = Workflow::default();
        let conversation_id = self
            .app
            .conversation_service()
            .create(dummy_workflow)
            .await?;

        // Dispatch the event to the conversation
        self.app
            .conversation_service()
            .insert_event(&conversation_id, event)
            .await
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
}
