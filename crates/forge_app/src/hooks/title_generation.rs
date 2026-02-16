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
        if let Some(handle) = self
            .title_handle
            .try_lock()
            .ok()
            .and_then(|mut guard| guard.take())
        {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        Agent, ChatCompletionMessage, Context, Conversation, ModelId, ProviderId, ToolCallContext,
        ToolCallFull, ToolResult,
    };
    use pretty_assertions::assert_eq;

    use super::*;

    /// Mock AgentService for testing
    #[derive(Clone)]
    struct MockAgentService;

    #[async_trait]
    impl AgentService for MockAgentService {
        async fn chat_agent(
            &self,
            _id: &ModelId,
            _context: Context,
            _provider_id: Option<ProviderId>,
        ) -> forge_domain::ResultStream<ChatCompletionMessage, anyhow::Error> {
            unreachable!("Not used in tests")
        }

        async fn call(
            &self,
            _agent: &Agent,
            _context: &ToolCallContext,
            _call: ToolCallFull,
        ) -> ToolResult {
            unreachable!("Not used in tests")
        }

        async fn update(&self, _conversation: Conversation) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn test_agent() -> Agent {
        Agent::new(
            "test-agent",
            "test-provider".to_string().into(),
            ModelId::new("test-model"),
        )
    }

    fn test_model_id() -> ModelId {
        ModelId::new("test-model")
    }

    fn test_event(user_prompt: &str) -> Event {
        Event::new(forge_domain::EventValue::text(user_prompt))
    }

    #[tokio::test]
    async fn test_start_skips_if_title_exists() {
        let fixture =
            TitleGenerationHandler::new(Arc::new(MockAgentService), test_event("Write a function"));
        let mut conversation = Conversation::generate().title(Some("Existing Title".to_string()));
        let event_data = EventData::new(test_agent(), test_model_id(), StartPayload);

        fixture
            .handle(&event_data, &mut conversation)
            .await
            .unwrap();

        // No task should have been spawned
        let guard = fixture.title_handle.lock().await;
        assert!(
            guard.is_none(),
            "No handle should be stored when title already exists"
        );
    }

    #[tokio::test]
    async fn test_start_skips_if_task_already_in_progress() {
        let fixture =
            TitleGenerationHandler::new(Arc::new(MockAgentService), test_event("Write a function"));
        let mut conversation = Conversation::generate();
        let event_data = EventData::new(test_agent(), test_model_id(), StartPayload);

        // Pre-inject a known handle to simulate an in-progress task
        let existing_handle = tokio::spawn(async { Some("Original".to_string()) });
        *fixture.title_handle.lock().await = Some(existing_handle);

        // Second start should not overwrite the existing handle
        fixture
            .handle(&event_data, &mut conversation)
            .await
            .unwrap();

        // The original handle should still be there and produce "Original"
        let handle = fixture
            .title_handle
            .lock()
            .await
            .take()
            .expect("Handle should still exist");
        let actual = handle.await.unwrap();
        let expected = Some("Original".to_string());
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_end_sets_title_from_completed_task() {
        let fixture =
            TitleGenerationHandler::new(Arc::new(MockAgentService), test_event("Test prompt"));

        // Inject a completed task handle that returns a title
        let title_handle = tokio::spawn(async { Some("Generated Title".to_string()) });
        *fixture.title_handle.lock().await = Some(title_handle);

        let mut conversation = Conversation::generate();
        let end_event = EventData::new(test_agent(), test_model_id(), EndPayload);

        fixture.handle(&end_event, &mut conversation).await.unwrap();

        let expected = Some("Generated Title".to_string());
        assert_eq!(conversation.title, expected);
    }

    #[tokio::test]
    async fn test_end_handles_task_failure() {
        let fixture =
            TitleGenerationHandler::new(Arc::new(MockAgentService), test_event("Test prompt"));

        // Inject a task handle that panics (simulates JoinError)
        let title_handle: JoinHandle<Option<String>> =
            tokio::spawn(async { panic!("task failure") });
        *fixture.title_handle.lock().await = Some(title_handle);

        let mut conversation = Conversation::generate();
        let end_event = EventData::new(test_agent(), test_model_id(), EndPayload);

        // Should not propagate the error, just log it
        fixture.handle(&end_event, &mut conversation).await.unwrap();

        assert!(conversation.title.is_none());
    }

    #[tokio::test]
    async fn test_drop_aborts_pending_task() {
        let fixture =
            TitleGenerationHandler::new(Arc::new(MockAgentService), test_event("Test prompt"));

        // Inject a long-running task
        let title_handle: JoinHandle<Option<String>> = tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            Some("Should never complete".to_string())
        });
        let abort_handle = title_handle.abort_handle();
        *fixture.title_handle.lock().await = Some(title_handle);

        drop(fixture);

        // Yield to let the runtime process the abort
        tokio::task::yield_now().await;
        assert!(abort_handle.is_finished());
    }
}
