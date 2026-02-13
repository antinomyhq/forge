use std::sync::Arc;

use async_trait::async_trait;
use forge_domain::{Conversation, EndPayload, EventData, EventHandle, StartPayload};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::agent::AgentService;
use crate::title_generator::TitleGenerator;

/// Hook handler that generates a conversation title asynchronously
#[derive(Clone)]
pub struct TitleGenerationHandler<S> {
    services: Arc<S>,
    title_handle: Arc<Mutex<Option<JoinHandle<Option<String>>>>>,
}

impl<S> TitleGenerationHandler<S> {
    /// Creates a new title generation handler
    pub fn new(services: Arc<S>) -> Self {
        Self { services, title_handle: Arc::new(Mutex::new(None)) }
    }
}

#[async_trait]
impl<S: AgentService> EventHandle<EventData<StartPayload>> for TitleGenerationHandler<S> {
    async fn handle(
        &self,
        event: &EventData<StartPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        // Early return if title already exists
        if conversation.title.is_some() {
            return Ok(());
        }

        // Extract the first user message from the conversation context
        let Some(user_prompt) = conversation.context.as_ref().and_then(|ctx| {
            ctx.messages.iter().find_map(|entry| match &entry.message {
                forge_domain::ContextMessage::Text(text_msg)
                    if text_msg.has_role(forge_domain::Role::User) =>
                {
                    text_msg
                        .raw_content
                        .as_ref()
                        .and_then(|val| val.as_user_prompt().cloned())
                        .or(Some(text_msg.content.clone().into()))
                }
                _ => None,
            })
        }) else {
            return Ok(());
        };

        // Configure and spawn title generation task
        let generator = TitleGenerator::new(
            self.services.clone(),
            user_prompt,
            event.model_id.clone(),
            Some(event.agent.provider.clone()),
        )
        .reasoning(event.agent.reasoning.clone());

        let handle = tokio::spawn(async move { generator.generate().await.ok().flatten() });

        *self.title_handle.lock().await = Some(handle);

        Ok(())
    }
}

#[async_trait]
impl<S: AgentService> EventHandle<EventData<EndPayload>> for TitleGenerationHandler<S> {
    async fn handle(
        &self,
        _event: &EventData<EndPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let Some(handle) = self.title_handle.lock().await.take() else {
            return Ok(());
        };

        match handle.await {
            Ok(Some(title)) => {
                debug!(
                    conversation_id = %conversation.id,
                    title = %title,
                    "Title generated successfully"
                );
                conversation.title = Some(title);
            }
            Ok(None) => debug!("Title generation returned None"),
            Err(e) => debug!(error = %e, "Title generation task failed"),
        }

        Ok(())
    }
}
