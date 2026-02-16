use std::sync::Arc;

use async_trait::async_trait;
use forge_domain::{Conversation, EndPayload, Event, EventData, EventHandle, StartPayload};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::agent::AgentService;
use crate::title_generator::TitleGenerator;

/// Hook handler that generates a conversation title asynchronously
#[derive(Clone)]
pub struct TitleGenerationHandler<S> {
    services: Arc<S>,
    event: Event,
    title_handle: Arc<Mutex<Option<JoinHandle<Option<String>>>>>,
}

impl<S> TitleGenerationHandler<S> {
    /// Creates a new title generation handler
    pub fn new(services: Arc<S>, event: Event) -> Self {
        Self { services, event, title_handle: Arc::new(Mutex::new(None)) }
    }
}

#[async_trait]
impl<S: AgentService> EventHandle<EventData<StartPayload>> for TitleGenerationHandler<S> {
    async fn handle(
        &self,
        event: &EventData<StartPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let mut guard = self.title_handle.lock().await;
        // Early return if title or task already exists
        if conversation.title.is_some() || guard.is_some() {
            return Ok(());
        }

        // Extract user prompt from the event
        let Some(user_prompt) = self
            .event
            .value
            .as_ref()
            .and_then(|val| val.as_user_prompt().cloned())
        else {
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

        *guard = Some(handle);

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

impl<S> Drop for TitleGenerationHandler<S> {
    fn drop(&mut self) {
        if let Some(handle) = self.title_handle.try_lock().ok().and_then(|mut guard| guard.take()) {
            handle.abort();
        }
    }
}