use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_services::{ForgeServices, Infrastructure};
use forge_stream::MpscStream;
use serde_json::Value;

use crate::executor::ForgeExecutorService;
use crate::suggestion::ForgeSuggestionService;
use crate::API;

pub struct ForgeAPI<F> {
    app: Arc<F>,
    executor_service: ForgeExecutorService<F>,
    suggestion_service: ForgeSuggestionService<F>,
}

impl<F: Services + Infrastructure> ForgeAPI<F> {
    pub fn new(app: Arc<F>) -> Self {
        Self {
            app: app.clone(),
            executor_service: ForgeExecutorService::new(app.clone()),
            suggestion_service: ForgeSuggestionService::new(app.clone()),
        }
    }
}

impl ForgeAPI<ForgeServices<ForgeInfra>> {
    pub fn init(restricted: bool) -> Self {
        let infra = Arc::new(ForgeInfra::new(restricted));
        let app = Arc::new(ForgeServices::new(infra));
        ForgeAPI::new(app)
    }
}

#[async_trait::async_trait]
impl<F: Services + Infrastructure> API for ForgeAPI<F> {
    //FIXME: need to pass the directory path for suggestions
    async fn suggestions(&self) -> Result<Vec<File>> {
        // Call the suggestion service with no specific conversation ID
        self.suggestion_service.suggestions(None).await
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
    ) -> anyhow::Result<MpscStream<anyhow::Result<AgentMessage<ChatResponse>, anyhow::Error>>> {
        Ok(self.executor_service.chat(chat).await?)
    }

    async fn init(&self, path: PathBuf) -> anyhow::Result<ConversationId> {
        // Create a loader service to load the workflow from the path
        let loader_service = forge_services::ForgeLoaderService::new(self.app.clone());

        // Load the workflow from the given path
        let workflow = loader_service.load(Some(path.as_path())).await?;

        // Create a new conversation with the workflow and the path as CWD
        self.app.conversation_service().create(workflow, path).await
    }

    fn environment(&self) -> Environment {
        Services::environment_service(self.app.as_ref())
            .get_environment()
            .clone()
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
