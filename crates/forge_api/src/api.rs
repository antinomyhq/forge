use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use forge_app::{EnvironmentService, ForgeApp, Infrastructure};
use forge_domain::*;
use forge_infra::ForgeInfra;
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
}#[cfg(test)]
mod tests;

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
    
    fn retry(
        &self,
        conversation_id: ConversationId,
    ) -> anyhow::Result<MpscStream<Result<AgentMessage<ChatResponse>, anyhow::Error>>> {
        let app = self.app.clone();
        let executor_service = self.executor_service.clone();
        
        Ok(MpscStream::spawn(move |tx| async move {
            let conversation = match app.conversation_service().get(&conversation_id).await {
                Ok(Some(conversation)) => conversation,
                Ok(None) => {
                    tx.send(Err(anyhow::anyhow!("Conversation not found"))).await.unwrap();
                    return;
                }
                Err(e) => {
                    tx.send(Err(anyhow::anyhow!("Failed to get conversation: {}", e))).await.unwrap();
                    return;
                }
            };
            
            // Find the last user message event
            let last_user_event = match conversation.events.iter().rev().find(|event| event.name == "message") {
                Some(event) => event.clone(),
                None => {
                    tx.send(Err(anyhow::anyhow!("No message found to retry"))).await.unwrap();
                    return;
                }
            };
            
            // Create a new chat request with the last user message
            let chat_request = ChatRequest::new(last_user_event, conversation_id.clone());
            
            // Forward all messages from the executor to our sender
            match executor_service.chat(chat_request).await {
                Ok(mut stream) => {
                    while let Some(message) = stream.next().await {
                        if tx.send(message).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    tx.send(Err(e)).await.unwrap();
                }
            }
        }))
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
}
