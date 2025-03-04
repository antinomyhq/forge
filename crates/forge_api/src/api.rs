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

    async fn retry(
        &self,
        conversation_id: ConversationId,
    ) -> anyhow::Result<MpscStream<Result<AgentMessage<ChatResponse>, anyhow::Error>>> {
        // Get the original conversation
        let conversation = self
            .app
            .conversation_service()
            .get(&conversation_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Conversation not found"))?;

        // Find the last user task event
        let last_task = conversation
            .events
            .iter()
            .rev()
            .find(|event| event.name == "user_task_init" || event.name == "user_task_update")
            .or_else(|| {
                conversation
                    .events
                    .iter()
                    .rev()
                    .find(|event| event.name == "prompt")
            })
            .ok_or_else(|| anyhow::anyhow!("No task found in conversation"))?;

        // Initialize a new conversation with the same workflow
        let new_conversation_id: ConversationId = self.app.conversation_service().create(conversation.workflow.clone()).await?;

        // Create a new chat request with the last task content and new conversation ID
        let chat = ChatRequest { 
            event: last_task.clone(),
            conversation_id: new_conversation_id 
        };

        // Call the chat method with the new request
        Ok(self.executor_service.chat(chat).await?)
    }
}
