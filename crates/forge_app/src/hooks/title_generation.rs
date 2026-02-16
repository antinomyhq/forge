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
        let mut guard = self.title_handle.lock().await;
        
        // Early return if title or task already exists
        if conversation.title.is_some() || guard.is_some() {
            return Ok(());
        }

        let user_prompt = conversation
            .context
            .as_ref()
            .and_then(|c| {
                c.messages
                    .iter()
                    .find(|m| m.has_role(forge_domain::Role::User))
            })
            .and_then(|e| e.message.as_value())
            .and_then(|e| e.as_user_prompt());

        let Some(user_prompt) = user_prompt else {
            return Ok(());
        };

        // Configure and spawn title generation task
        let generator = TitleGenerator::new(
            self.services.clone(),
            user_prompt.clone(),
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
        Agent, ChatCompletionMessage, Context, ContextMessage, Conversation, EventValue, ModelId,
        ProviderId, Role, TextMessage, ToolCallContext, ToolCallFull, ToolResult,
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

    fn setup(message: &str) -> (TitleGenerationHandler<MockAgentService>, Conversation) {
        let handler = TitleGenerationHandler::new(Arc::new(MockAgentService));
        let context = Context::default().add_message(ContextMessage::Text(
            TextMessage::new(Role::User, message).raw_content(EventValue::text(message)),
        ));
        let conversation = Conversation::generate().context(context);
        (handler, conversation)
    }

    #[tokio::test]
    async fn test_start_skips_if_title_exists() {
        let (handler, mut conversation) = setup("test message");
        conversation.title = Some("existing".into());

        handler.handle(&EventData::new(Agent::new("t", "t".to_string().into(), ModelId::new("t")), ModelId::new("t"), StartPayload), &mut conversation).await.unwrap();

        assert!(handler.title_handle.lock().await.is_none());
    }

    #[tokio::test]
    async fn test_start_skips_if_task_already_in_progress() {
        let (handler, mut conversation) = setup("test message");
        *handler.title_handle.lock().await = Some(tokio::spawn(async { Some("original".into()) }));

        handler.handle(&EventData::new(Agent::new("t", "t".to_string().into(), ModelId::new("t")), ModelId::new("t"), StartPayload), &mut conversation).await.unwrap();

        let result = handler.title_handle.lock().await.take().unwrap().await.unwrap();
        assert_eq!(result, Some("original".into()));
    }

    #[tokio::test]
    async fn test_end_sets_title_from_completed_task() {
        let (handler, mut conversation) = setup("test message");
        *handler.title_handle.lock().await = Some(tokio::spawn(async { Some("generated".into()) }));

        handler.handle(&EventData::new(Agent::new("t", "t".to_string().into(), ModelId::new("t")), ModelId::new("t"), EndPayload), &mut conversation).await.unwrap();

        assert_eq!(conversation.title, Some("generated".into()));
    }

    #[tokio::test]
    async fn test_end_handles_task_failure() {
        let (handler, mut conversation) = setup("test message");
        *handler.title_handle.lock().await = Some(tokio::spawn(async { panic!("fail") }));

        handler.handle(&EventData::new(Agent::new("t", "t".to_string().into(), ModelId::new("t")), ModelId::new("t"), EndPayload), &mut conversation).await.unwrap();

        assert!(conversation.title.is_none());
    }

    #[tokio::test]
    async fn test_drop_aborts_pending_task() {
        let (handler, _) = setup("test message");
        let handle = tokio::spawn(async { tokio::time::sleep(tokio::time::Duration::from_secs(60)).await; Some("x".into()) });
        let abort = handle.abort_handle();
        *handler.title_handle.lock().await = Some(handle);

        drop(handler);

        tokio::task::yield_now().await;
        assert!(abort.is_finished());
    }
}
