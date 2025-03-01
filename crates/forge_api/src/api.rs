use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use forge_app::{EnvironmentService, ForgeApp, Infrastructure};
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_stream::MpscStream;

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
        // Initialize a new conversation with default workflow for the event
        let conversation_id = self.init(self.load(None).await?).await?;

        // Use the executor service to handle the event dispatch
        let chat = ChatRequest {
            content: format!(
                "Dispatching event: {} with value: {}",
                event_name, event_value
            ),
            conversation_id,
        };

        // Execute the chat request which will handle the event
        let mut stream = self.executor_service.chat(chat).await?;

        // Process the stream to ensure the event is handled
        use futures::StreamExt;
        while let Some(result) = stream.next().await {
            result?;
        }

        Ok(())
    }
}
