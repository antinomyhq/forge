use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;

use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_services::{ForgeServices, Infrastructure};
use forge_stream::MpscStream;

use crate::API;
use crate::executor::ForgeExecutorService;
use crate::loader::ForgeLoaderService;
use crate::suggestion::ForgeSuggestionService;

pub struct ForgeAPI<F> {
    app: Arc<F>,
    executor_service: ForgeExecutorService<F>,
    suggestion_service: ForgeSuggestionService<F>,
    loader: ForgeLoaderService<F>,
}

impl<F: Services + Infrastructure> ForgeAPI<F> {
    pub fn new(app: Arc<F>, workflow_path: Option<PathBuf>) -> Self {
        Self {
            app: app.clone(),
            executor_service: ForgeExecutorService::new(app.clone()),
            suggestion_service: ForgeSuggestionService::new(app.clone()),
            loader: ForgeLoaderService::new(app.clone(), workflow_path),
        }
    }
}

impl ForgeAPI<ForgeServices<ForgeInfra>> {
    pub fn init(restricted: bool, workflow_path: Option<PathBuf>) -> Self {
        let infra = Arc::new(ForgeInfra::new(restricted));
        let app = Arc::new(ForgeServices::new(infra));
        ForgeAPI::new(app, workflow_path)
    }
}

#[async_trait::async_trait]
impl<F: Services + Infrastructure> API for ForgeAPI<F> {
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

    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.app.conversation_service().upsert(conversation).await
    }

    fn environment(&self) -> Environment {
        Services::environment_service(self.app.as_ref())
            .get_environment()
            .clone()
    }

    async fn load(&self) -> anyhow::Result<Workflow> {
        self.loader.load().await
    }

    async fn conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>> {
        self.app.conversation_service().find(conversation_id).await
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
