use std::sync::Arc;

use async_trait::async_trait;
use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use forge_domain::{
    Conversation, ConversationId, EndPayload, EventData, EventHandle, StartPayload,
};
use tokio::task::JoinHandle;
use tracing::debug;

use crate::agent::AgentService;
use crate::title_generator::TitleGenerator;

/// Per-conversation title generation state.
enum TitleTask {
    /// A background task is running; handle is owned by the map.
    InProgress(JoinHandle<Option<String>>),
    /// `EndPayload` has extracted the handle and is currently awaiting it.
    /// Kept in the map as a sentinel so a concurrent `StartPayload` sees an
    /// occupied entry and does not spawn a duplicate task.
    Awaiting,
    /// Title generation has finished successfully; stores the generated title.
    Done(#[allow(dead_code)] String),
}

/// Hook handler that generates a conversation title asynchronously.
#[derive(Clone)]
pub struct TitleGenerationHandler<S> {
    services: Arc<S>,
    title_tasks: Arc<DashMap<ConversationId, TitleTask>>,
}

impl<S> TitleGenerationHandler<S> {
    /// Creates a new title generation handler.
    pub fn new(services: Arc<S>) -> Self {
        Self { services, title_tasks: Arc::new(DashMap::new()) }
    }
}

#[async_trait]
impl<S: AgentService> EventHandle<EventData<StartPayload>> for TitleGenerationHandler<S> {
    async fn handle(
        &self,
        event: &EventData<StartPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        if conversation.title.is_some() {
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

        let generator = TitleGenerator::new(
            self.services.clone(),
            user_prompt.clone(),
            event.model_id.clone(),
            Some(event.agent.provider.clone()),
        )
        .reasoning(event.agent.reasoning.clone());

        // `or_insert_with` holds the shard lock for its entire call. Any occupied
        // entry — InProgress, Awaiting, or Done — is left untouched, so at most
        // one task is ever spawned per conversation id.
        self.title_tasks
            .entry(conversation.id)
            .or_insert_with(|| {
                TitleTask::InProgress(
                    tokio::spawn(async move { generator.generate().await.ok().flatten() }),
                )
            });

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
        // Atomically transition InProgress → Awaiting, extracting the handle while
        // keeping the entry occupied. A concurrent StartPayload sees Occupied and
        // skips, so no duplicate task can be spawned during the await below.
        let handle = match self.title_tasks.entry(conversation.id) {
            Entry::Occupied(mut e) => {
                match std::mem::replace(e.get_mut(), TitleTask::Awaiting) {
                    TitleTask::InProgress(h) => h,
                    // Awaiting or Done: another EndPayload is already handling this.
                    TitleTask::Done(title) => {
                        conversation.title = Some(title);
                        return Ok(());
                    }
                    other => {
                        *e.get_mut() = other; // restore
                        return Ok(());
                    }
                }
            }
            Entry::Vacant(_) => return Ok(()),
        };

        match handle.await {
            Ok(Some(title)) => {
                debug!(
                    conversation_id = %conversation.id,
                    title = %title,
                    "Title generated successfully"
                );
                conversation.title = Some(title.clone());
                // Transition Awaiting → Done only on success.
                self.title_tasks.insert(conversation.id, TitleTask::Done(title));
            }
            Ok(None) => {
                debug!("Title generation returned None");
                // Remove so a future StartPayload can retry.
                self.title_tasks.remove(&conversation.id);
            }
            Err(e) => {
                debug!(error = %e, "Title generation task failed");
                // Remove so a future StartPayload can retry.
                self.title_tasks.remove(&conversation.id);
            }
        }

        Ok(())
    }
}

impl<S> Drop for TitleGenerationHandler<S> {
    fn drop(&mut self) {
        self.title_tasks.retain(|_, task| {
            if let TitleTask::InProgress(handle) = task {
                handle.abort();
            }
            false
        });
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
            Ok(Box::pin(futures::stream::empty()))
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

    fn event<T: Send + Sync>(payload: T) -> EventData<T> {
        EventData::new(
            Agent::new("t", "t".to_string().into(), ModelId::new("t")),
            ModelId::new("t"),
            payload,
        )
    }

    #[tokio::test]
    async fn test_start_skips_if_title_exists() {
        let (handler, mut conversation) = setup("test message");
        conversation.title = Some("existing".into());

        handler.handle(&event(StartPayload), &mut conversation).await.unwrap();

        assert!(!handler.title_tasks.contains_key(&conversation.id));
    }

    #[tokio::test]
    async fn test_start_skips_if_task_already_in_progress() {
        let (handler, mut conversation) = setup("test message");
        let original = tokio::spawn(async { Some("original".into()) });
        handler.title_tasks.insert(conversation.id, TitleTask::InProgress(original));

        handler.handle(&event(StartPayload), &mut conversation).await.unwrap();

        let (_, task) = handler.title_tasks.remove(&conversation.id).unwrap();
        let actual = match task {
            TitleTask::InProgress(h) => h.await.unwrap(),
            _ => panic!("Expected InProgress"),
        };
        assert_eq!(actual, Some("original".into()));
    }

    /// A StartPayload that races with an EndPayload mid-await must not spawn a
    /// new task — the Awaiting sentinel keeps the entry occupied.
    #[tokio::test]
    async fn test_start_skips_if_awaiting() {
        let (handler, mut conversation) = setup("test message");
        handler.title_tasks.insert(conversation.id, TitleTask::Awaiting);

        handler.handle(&event(StartPayload), &mut conversation).await.unwrap();

        assert!(matches!(
            handler.title_tasks.get(&conversation.id).as_deref(),
            Some(TitleTask::Awaiting)
        ));
    }

    /// A StartPayload after generation has finished must not re-spawn.
    #[tokio::test]
    async fn test_start_skips_if_done() {
        let (handler, mut conversation) = setup("test message");
        handler.title_tasks.insert(conversation.id, TitleTask::Done("existing".into()));

        handler.handle(&event(StartPayload), &mut conversation).await.unwrap();

        assert!(matches!(
            handler.title_tasks.get(&conversation.id).as_deref(),
            Some(TitleTask::Done(_))
        ));
    }

    #[tokio::test]
    async fn test_end_sets_title_from_completed_task() {
        let (handler, mut conversation) = setup("test message");
        handler.title_tasks.insert(
            conversation.id,
            TitleTask::InProgress(tokio::spawn(async { Some("generated".into()) })),
        );

        handler.handle(&event(EndPayload), &mut conversation).await.unwrap();

        assert_eq!(conversation.title, Some("generated".into()));
        assert!(matches!(
            handler.title_tasks.get(&conversation.id).as_deref(),
            Some(TitleTask::Done(_))
        ));
    }

    #[tokio::test]
    async fn test_end_handles_task_failure() {
        let (handler, mut conversation) = setup("test message");
        handler.title_tasks.insert(
            conversation.id,
            TitleTask::InProgress(tokio::spawn(async { panic!("fail") })),
        );

        handler.handle(&event(EndPayload), &mut conversation).await.unwrap();

        assert!(conversation.title.is_none());
        assert!(!handler.title_tasks.contains_key(&conversation.id));
    }

    /// Many concurrent StartPayload calls for the same conversation id must
    /// result in exactly one spawned task.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_start_spawns_only_one_task() {
        let (handler, conversation) = setup("test message");
        let barrier = Arc::new(tokio::sync::Barrier::new(20));
        let handler = Arc::new(handler);

        let mut joins = Vec::new();
        for _ in 0..20 {
            let handler = handler.clone();
            let barrier = barrier.clone();
            let mut conv = conversation.clone();
            joins.push(tokio::spawn(async move {
                barrier.wait().await;
                handler.handle(&event(StartPayload), &mut conv).await.unwrap();
            }));
        }
        for j in joins {
            j.await.unwrap();
        }

        let actual = handler
            .title_tasks
            .iter()
            .filter(|e| matches!(e.value(), TitleTask::InProgress(_)))
            .count();
        assert_eq!(actual, 1);
    }

    /// A StartPayload that arrives while EndPayload is mid-await (Awaiting
    /// sentinel) must not spawn a new task.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_concurrent_start_during_end_await_does_not_respawn() {
        let (handler, mut conversation) = setup("test message");
        let handler = Arc::new(handler);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();

        // slow_handle just blocks on rx; no barrier needed here.
        let slow_handle = tokio::spawn(async move {
            rx.await.ok();
            Some("title".to_string())
        });
        handler.title_tasks.insert(conversation.id, TitleTask::InProgress(slow_handle));

        let end_task = tokio::spawn({
            let handler = handler.clone();
            let cid = conversation.id;
            async move {
                let mut conv = Conversation::generate();
                conv.id = cid;
                handler.handle(&event(EndPayload), &mut conv).await.unwrap();
                conv.title
            }
        });

        // Spin-yield until end_task has actually transitioned InProgress → Awaiting.
        // This avoids the race where slow_handle (independently scheduled) could
        // release a barrier before end_task has even started.
        while !matches!(
            handler.title_tasks.get(&conversation.id).as_deref(),
            Some(TitleTask::Awaiting)
        ) {
            tokio::task::yield_now().await;
        }

        // Now fire StartPayload — must see Awaiting and skip.
        handler.handle(&event(StartPayload), &mut conversation).await.unwrap();
        assert!(matches!(
            handler.title_tasks.get(&conversation.id).as_deref(),
            Some(TitleTask::Awaiting)
        ));

        tx.send(()).unwrap();
        let actual = end_task.await.unwrap();
        assert_eq!(actual, Some("title".to_string()));
        assert!(matches!(
            handler.title_tasks.get(&conversation.id).as_deref(),
            Some(TitleTask::Done(_))
        ));
    }
}
