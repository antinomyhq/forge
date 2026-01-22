use std::fmt;

use async_trait::async_trait;
use derive_setters::Setters;

use crate::{
    Agent, ChatCompletionMessageFull, Conversation, InterruptionReason, ModelId, ToolCallFull,
    ToolResult,
};

/// Lifecycle events that can occur during conversation processing
#[derive(Debug, PartialEq, Clone)]
pub enum LifecycleEvent {
    /// Event fired when conversation processing starts
    ///
    /// Contains the model ID being used
    Start { agent: Agent, model_id: ModelId },

    /// Event fired when conversation processing ends
    End,

    /// Event fired when a request is made to the LLM
    ///
    /// Contains the model ID and request count
    Request {
        agent: Agent,
        model_id: ModelId,
        request_count: usize,
    },

    /// Event fired when a response is received from the LLM
    ///
    /// Contains the full response message
    Response(ChatCompletionMessageFull),

    /// Event fired when a tool call starts
    ToolcallStart(ToolCallFull),

    /// Event fired when a tool call ends
    ToolcallEnd(ToolResult),
}

/// Represents a step in the conversation processing pipeline
///
/// This enum is open for extension - new variants can be added to represent
/// different control flow decisions in the processing pipeline.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Continue processing
    #[default]
    Proceed,
    /// Halt processing with a reason
    Interrupt { reason: InterruptionReason },
}

impl Step {
    /// Creates a Continue step
    pub fn proceed() -> Self {
        Self::Proceed
    }

    /// Creates a Halt step with a reason
    pub fn interrupt(reason: impl Into<InterruptionReason>) -> Self {
        Self::Interrupt { reason: reason.into() }
    }

    /// Returns true if this step indicates processing should continue
    pub fn should_proceed(&self) -> bool {
        matches!(self, Self::Proceed)
    }

    /// Returns true if this step indicates processing should halt
    pub fn should_interrupt(&self) -> bool {
        matches!(self, Self::Interrupt { .. })
    }

    /// Returns the reason if this is a interrupt step
    pub fn reason(&self) -> Option<&InterruptionReason> {
        match self {
            Self::Interrupt { reason } => Some(reason),
            Self::Proceed => None,
        }
    }
}

/// Trait for handling lifecycle events
///
/// Implementations of this trait can be used to react to different
/// stages of conversation processing.
#[async_trait]
pub trait EventHandle: Send + Sync {
    /// Handles a lifecycle event and potentially modifies the conversation
    ///
    /// # Arguments
    /// * `event` - The lifecycle event that occurred
    /// * `conversation` - The current conversation state (mutable)
    ///
    /// # Returns
    /// A step indicating how to proceed
    ///
    /// # Errors
    /// Returns an error if the event handling fails
    async fn handle(
        &self,
        event: LifecycleEvent,
        conversation: &mut Conversation,
    ) -> anyhow::Result<Step>;
}

/// Extension trait for combining event handlers
///
/// This trait provides methods to combine multiple event handlers into a single
/// handler that executes them in sequence with short-circuit behavior.
pub trait EventHandleExt: EventHandle {
    /// Combines this handler with another handler, creating a new handler that
    /// runs both in sequence with short-circuit behavior
    ///
    /// When an event is handled, this handler runs first. If it returns
    /// `Step::Proceed`, the other handler runs and its result is returned.
    /// If this handler returns `Step::Interrupt`, the other handler is skipped
    /// and the interrupt is returned immediately (short-circuit).
    ///
    /// **Important**: If you need the second handler to always execute (e.g.,
    /// for cleanup, logging, or metrics), do not rely on this method.
    /// Instead, implement a single handler that performs both operations.
    ///
    /// # Arguments
    /// * `other` - Another handler to combine with this one
    ///
    /// # Returns
    /// A new boxed handler that combines both handlers
    fn and<H: EventHandle + 'static>(self, other: H) -> Box<dyn EventHandle>
    where
        Self: Sized + 'static;
}

impl<T: EventHandle + 'static> EventHandleExt for T {
    fn and<H: EventHandle + 'static>(self, other: H) -> Box<dyn EventHandle>
    where
        Self: Sized + 'static,
    {
        Box::new(CombinedHandler(Box::new(self), Box::new(other)))
    }
}

// Implement EventHandle for Box<dyn EventHandle> to allow using boxed handlers
#[async_trait]
impl EventHandle for Box<dyn EventHandle> {
    async fn handle(
        &self,
        event: LifecycleEvent,
        conversation: &mut Conversation,
    ) -> anyhow::Result<Step> {
        (**self).handle(event, conversation).await
    }
}

/// A hook that contains handlers for all lifecycle events
///
/// Hooks allow you to attach custom behavior at specific points
/// during conversation processing.
#[derive(Setters)]
#[setters(into)]
pub struct Hook {
    on_start: Box<dyn EventHandle>,
    on_end: Box<dyn EventHandle>,
    on_request: Box<dyn EventHandle>,
    on_response: Box<dyn EventHandle>,
    on_toolcall_start: Box<dyn EventHandle>,
    on_toolcall_end: Box<dyn EventHandle>,
}

impl fmt::Debug for Hook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hook")
            .field("on_start", &"<handler>")
            .field("on_end", &"<handler>")
            .field("on_request", &"<handler>")
            .field("on_response", &"<handler>")
            .field("on_toolcall_start", &"<handler>")
            .field("on_toolcall_end", &"<handler>")
            .finish()
    }
}

impl Hook {
    /// Creates a new hook with the provided event handlers
    pub fn new(
        on_start: impl Into<Box<dyn EventHandle>>,
        on_end: impl Into<Box<dyn EventHandle>>,
        on_request: impl Into<Box<dyn EventHandle>>,
        on_response: impl Into<Box<dyn EventHandle>>,
        on_toolcall_start: impl Into<Box<dyn EventHandle>>,
        on_toolcall_end: impl Into<Box<dyn EventHandle>>,
    ) -> Self {
        Self {
            on_start: on_start.into(),
            on_end: on_end.into(),
            on_request: on_request.into(),
            on_response: on_response.into(),
            on_toolcall_start: on_toolcall_start.into(),
            on_toolcall_end: on_toolcall_end.into(),
        }
    }

    /// Creates a new hook with all no-op handlers
    ///
    /// This is useful when you need a hook but don't want to handle any events.
    pub fn default() -> Self {
        Self {
            on_start: Box::new(NoOpHandler),
            on_end: Box::new(NoOpHandler),
            on_request: Box::new(NoOpHandler),
            on_response: Box::new(NoOpHandler),
            on_toolcall_start: Box::new(NoOpHandler),
            on_toolcall_end: Box::new(NoOpHandler),
        }
    }

    /// Combines this hook with another hook, creating a new hook that runs both
    /// handlers with short-circuit behavior
    ///
    /// When an event is handled, the first hook's handler runs first. If it
    /// returns `Step::Proceed`, the second hook's handler runs and its
    /// result is returned. If the first handler returns `Step::Interrupt`,
    /// the second handler is skipped and the interrupt is returned
    /// immediately (short-circuit).
    ///
    /// **Important**: If you need the second hook's handlers to always execute
    /// (e.g., for cleanup, logging, or metrics), do not rely on this
    /// method. Instead, implement a single hook that performs both
    /// operations.
    ///
    /// # Arguments
    /// * `other` - Another hook to combine with this one
    ///
    /// # Returns
    /// A new hook that combines both hooks' handlers
    pub fn zip(self, other: Hook) -> Self {
        Self {
            on_start: self.on_start.and(other.on_start),
            on_end: self.on_end.and(other.on_end),
            on_request: self.on_request.and(other.on_request),
            on_response: self.on_response.and(other.on_response),
            on_toolcall_start: self.on_toolcall_start.and(other.on_toolcall_start),
            on_toolcall_end: self.on_toolcall_end.and(other.on_toolcall_end),
        }
    }
}

// Implement EventHandle for Hook to allow hooks to be used as handlers
#[async_trait]
impl EventHandle for Hook {
    async fn handle(
        &self,
        event: LifecycleEvent,
        conversation: &mut Conversation,
    ) -> anyhow::Result<Step> {
        match event {
            LifecycleEvent::Start { agent: _, model_id: _ } => {
                self.on_start.handle(event, conversation).await
            }
            LifecycleEvent::End => self.on_end.handle(event, conversation).await,
            LifecycleEvent::Request { agent: _, model_id: _, request_count: _ } => {
                self.on_request.handle(event, conversation).await
            }
            LifecycleEvent::Response(_) => self.on_response.handle(event, conversation).await,
            LifecycleEvent::ToolcallStart(_) => {
                self.on_toolcall_start.handle(event, conversation).await
            }
            LifecycleEvent::ToolcallEnd(_) => {
                self.on_toolcall_end.handle(event, conversation).await
            }
        }
    }
}

/// A handler that combines two event handlers with short-circuit behavior
///
/// Runs the first handler, and only runs the second handler if the first
/// returns `Step::Proceed`. If the first handler returns `Step::Interrupt`, the
/// second handler is skipped and the interrupt is returned immediately.
///
/// This is used internally by the `Hook::zip` and `EventHandleExt::and`
/// methods.
struct CombinedHandler(Box<dyn EventHandle>, Box<dyn EventHandle>);

#[async_trait]
impl EventHandle for CombinedHandler {
    async fn handle(
        &self,
        event: LifecycleEvent,
        conversation: &mut Conversation,
    ) -> anyhow::Result<Step> {
        // Run the first handler
        let step = self.0.handle(event.clone(), conversation).await?;
        match step {
            Step::Proceed => {
                // Run the second handler with the cloned event
                self.1.handle(event, conversation).await
            }
            Step::Interrupt { .. } => Ok(step),
        }
    }
}

/// A no-op handler that does nothing
///
/// This is useful as a default handler when you only want to
/// handle specific events.
#[derive(Debug, Default)]
pub struct NoOpHandler;

#[async_trait]
impl EventHandle for NoOpHandler {
    async fn handle(
        &self,
        _event: LifecycleEvent,
        _conversation: &mut Conversation,
    ) -> anyhow::Result<Step> {
        Ok(Step::proceed())
    }
}

#[async_trait]
impl<F, Fut> EventHandle for F
where
    F: Fn(LifecycleEvent, &mut Conversation) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = anyhow::Result<Step>> + Send,
{
    async fn handle(
        &self,
        event: LifecycleEvent,
        conversation: &mut Conversation,
    ) -> anyhow::Result<Step> {
        (self)(event, conversation).await
    }
}

impl<F, Fut> From<F> for Box<dyn EventHandle>
where
    F: Fn(LifecycleEvent, &mut Conversation) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = anyhow::Result<Step>> + Send + 'static,
{
    fn from(handler: F) -> Self {
        Box::new(handler)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{Agent, AgentId, Conversation, ModelId, ProviderId};

    fn test_agent() -> Agent {
        Agent::new(
            AgentId::new("test_agent"),
            ProviderId::FORGE,
            ModelId::new("test-model"),
        )
    }

    fn test_model_id() -> ModelId {
        ModelId::new("test-model")
    }

    #[test]
    fn test_no_op_handler() {
        let handler = NoOpHandler;
        let conversation = Conversation::generate();

        // This test just ensures NoOpHandler compiles and is constructible
        let _ = handler;
        let _ = conversation;
    }

    #[test]
    fn test_step_continue() {
        let step = Step::proceed();

        assert!(step.should_proceed());
        assert!(!step.should_interrupt());
    }

    #[test]
    fn test_step_halt() {
        let step = Step::interrupt(InterruptionReason::MaxRequestPerTurnLimitReached { limit: 10 });

        assert!(!step.should_proceed());
        assert!(step.should_interrupt());
        assert_eq!(
            step.reason(),
            Some(&InterruptionReason::MaxRequestPerTurnLimitReached { limit: 10 })
        );
    }

    #[test]
    fn test_step_from_conversation() {
        let step = Step::proceed();

        assert!(step.should_proceed());
    }

    #[test]
    fn test_step_into_conversation() {
        let step = Step::proceed();

        // Just verify it's a Continue step
        assert!(step.should_proceed());
    }

    #[test]
    fn test_step_conversation_mut() {
        let step = Step::proceed();

        // Just verify it's a Continue step
        assert!(step.should_proceed());
    }

    #[tokio::test]
    async fn test_hook_on_start() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let hook = Hook::default().on_start(
            move |event: LifecycleEvent, _conversation: &mut Conversation| {
                let events = events_clone.clone();
                async move {
                    events.lock().unwrap().push(event);
                    Ok(Step::proceed())
                }
            },
        );

        let mut conversation = Conversation::generate();

        let step = hook
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();
        assert!(step.should_proceed());

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 1);
        assert_eq!(
            handled[0],
            LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() }
        );
    }

    #[tokio::test]
    async fn test_hook_builder() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let hook = Hook::default()
            .on_start({
                let events = events.clone();
                move |event: LifecycleEvent, _conversation: &mut Conversation| {
                    let events = events.clone();
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(Step::proceed())
                    }
                }
            })
            .on_end({
                let events = events.clone();
                move |event: LifecycleEvent, _conversation: &mut Conversation| {
                    let events = events.clone();
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(Step::proceed())
                    }
                }
            })
            .on_request({
                let events = events.clone();
                move |event: LifecycleEvent, _conversation: &mut Conversation| {
                    let events = events.clone();
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(Step::proceed())
                    }
                }
            });

        let mut conversation = Conversation::generate();

        // Test Start event
        let _ = hook
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();
        // Test End event
        let _ = hook
            .handle(LifecycleEvent::End, &mut conversation)
            .await
            .unwrap();
        // Test Request event
        let _ = hook
            .handle(
                LifecycleEvent::Request {
                    agent: test_agent(),
                    model_id: test_model_id(),
                    request_count: 1,
                },
                &mut conversation,
            )
            .await
            .unwrap();

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 3);
        assert_eq!(
            handled[0],
            LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() }
        );
        assert_eq!(handled[1], LifecycleEvent::End);
        assert_eq!(
            handled[2],
            LifecycleEvent::Request {
                agent: test_agent(),
                model_id: test_model_id(),
                request_count: 1,
            }
        );
    }

    #[tokio::test]
    async fn test_hook_all_events() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let hook = Hook::new(
            {
                let events = events.clone();
                move |event: LifecycleEvent, _conversation: &mut Conversation| {
                    let events = events.clone();
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(Step::proceed())
                    }
                }
            },
            {
                let events = events.clone();
                move |event: LifecycleEvent, _conversation: &mut Conversation| {
                    let events = events.clone();
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(Step::proceed())
                    }
                }
            },
            {
                let events = events.clone();
                move |event: LifecycleEvent, _conversation: &mut Conversation| {
                    let events = events.clone();
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(Step::proceed())
                    }
                }
            },
            {
                let events = events.clone();
                move |event: LifecycleEvent, _conversation: &mut Conversation| {
                    let events = events.clone();
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(Step::proceed())
                    }
                }
            },
            {
                let events = events.clone();
                move |event: LifecycleEvent, _conversation: &mut Conversation| {
                    let events = events.clone();
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(Step::proceed())
                    }
                }
            },
            {
                let events = events.clone();
                move |event: LifecycleEvent, _conversation: &mut Conversation| {
                    let events = events.clone();
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(Step::proceed())
                    }
                }
            },
        );

        let mut conversation = Conversation::generate();

        let all_events = vec![
            LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
            LifecycleEvent::End,
            LifecycleEvent::Request {
                agent: test_agent(),
                model_id: test_model_id(),
                request_count: 1,
            },
            LifecycleEvent::Response(ChatCompletionMessageFull {
                content: "test".to_string(),
                reasoning: None,
                tool_calls: vec![],
                reasoning_details: None,
                usage: crate::Usage::default(),
                finish_reason: None,
            }),
            LifecycleEvent::ToolcallStart(ToolCallFull::new("test_tool")),
            LifecycleEvent::ToolcallEnd(ToolResult::new("test_tool")),
        ];

        for event in all_events {
            let _ = hook.handle(event, &mut conversation).await.unwrap();
        }

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 6);
    }

    #[tokio::test]
    async fn test_step_mutable_conversation() {
        let title = std::sync::Arc::new(std::sync::Mutex::new(None));
        let hook = Hook::default().on_start({
            let title = title.clone();
            move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                let title = title.clone();
                async move {
                    *title.lock().unwrap() = Some("Modified title".to_string());
                    Ok(Step::proceed())
                }
            }
        });
        let mut conversation = Conversation::generate();

        assert!(title.lock().unwrap().is_none());

        let step = hook
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();

        assert!(step.should_proceed());
        assert_eq!(*title.lock().unwrap(), Some("Modified title".to_string()));
    }

    #[tokio::test]
    async fn test_step_halt_variant() {
        let hook = Hook::default().on_start(
            |_event: LifecycleEvent, _conversation: &mut Conversation| async move {
                Ok(Step::interrupt(
                    InterruptionReason::MaxRequestPerTurnLimitReached { limit: 5 },
                ))
            },
        );

        let mut conversation = Conversation::generate();

        let step = hook
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();

        assert!(step.should_interrupt());
        assert!(!step.should_proceed());
        assert_eq!(
            step.reason(),
            Some(&InterruptionReason::MaxRequestPerTurnLimitReached { limit: 5 })
        );
    }

    #[test]
    fn test_hook_default() {
        let hook = Hook::default();

        // Just ensure it compiles and is constructible
        let _ = hook;
    }

    #[tokio::test]
    async fn test_hook_zip() {
        let counter1 = std::sync::Arc::new(std::sync::Mutex::new(0));
        let counter2 = std::sync::Arc::new(std::sync::Mutex::new(0));

        let hook1 = Hook::default().on_start({
            let counter = counter1.clone();
            move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(Step::proceed())
                }
            }
        });

        let hook2 = Hook::default().on_start({
            let counter = counter2.clone();
            move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(Step::proceed())
                }
            }
        });
        let combined: Hook = hook1.zip(hook2);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();

        // Both handlers should have been called
        assert_eq!(*counter1.lock().unwrap(), 1);
        assert_eq!(*counter2.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_hook_zip_multiple() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let hook1 = Hook::default().on_start({
            let events = events.clone();
            move |event: LifecycleEvent, _conversation: &mut Conversation| {
                let events = events.clone();
                async move {
                    events.lock().unwrap().push(format!("h1:{:?}", event));
                    Ok(Step::proceed())
                }
            }
        });

        let hook2 = Hook::default().on_start({
            let events = events.clone();
            move |event: LifecycleEvent, _conversation: &mut Conversation| {
                let events = events.clone();
                async move {
                    events.lock().unwrap().push(format!("h2:{:?}", event));
                    Ok(Step::proceed())
                }
            }
        });

        let hook3 = Hook::default().on_start({
            let events = events.clone();
            move |event: LifecycleEvent, _conversation: &mut Conversation| {
                let events = events.clone();
                async move {
                    events.lock().unwrap().push(format!("h3:{:?}", event));
                    Ok(Step::proceed())
                }
            }
        });
        let combined: Hook = hook1.zip(hook2).zip(hook3);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 3);
        assert!(handled[0].starts_with("h1:Start"));
        assert!(handled[1].starts_with("h2:Start"));
        assert!(handled[2].starts_with("h3:Start"));
    }

    #[tokio::test]
    async fn test_hook_zip_different_events() {
        let start_title = std::sync::Arc::new(std::sync::Mutex::new(None));
        let end_title = std::sync::Arc::new(std::sync::Mutex::new(None));

        let hook1 = Hook::default()
            .on_start({
                let start_title = start_title.clone();
                move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                    let start_title = start_title.clone();
                    async move {
                        *start_title.lock().unwrap() = Some("Start".to_string());
                        Ok(Step::proceed())
                    }
                }
            })
            .on_end({
                let end_title = end_title.clone();
                move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                    let end_title = end_title.clone();
                    async move {
                        *end_title.lock().unwrap() = Some("End".to_string());
                        Ok(Step::proceed())
                    }
                }
            });
        let hook2 = Hook::default();

        let combined: Hook = hook1.zip(hook2);

        let mut conversation = Conversation::generate();

        // Test Start event
        let _ = combined
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();
        assert_eq!(*start_title.lock().unwrap(), Some("Start".to_string()));

        // Test End event
        let _ = combined
            .handle(LifecycleEvent::End, &mut conversation)
            .await
            .unwrap();
        assert_eq!(*end_title.lock().unwrap(), Some("End".to_string()));
    }

    #[tokio::test]
    async fn test_event_handle_ext_and() {
        let counter1 = std::sync::Arc::new(std::sync::Mutex::new(0));
        let counter2 = std::sync::Arc::new(std::sync::Mutex::new(0));

        let handler1 = {
            let counter = counter1.clone();
            move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(Step::proceed())
                }
            }
        };

        let handler2 = {
            let counter = counter2.clone();
            move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(Step::proceed())
                }
            }
        };

        let combined: Box<dyn EventHandle> = handler1.and(handler2);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();

        // Both handlers should have been called
        assert_eq!(*counter1.lock().unwrap(), 1);
        assert_eq!(*counter2.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_event_handle_ext_and_boxed() {
        let counter1 = std::sync::Arc::new(std::sync::Mutex::new(0));
        let counter2 = std::sync::Arc::new(std::sync::Mutex::new(0));

        let handler1 = {
            let counter = counter1.clone();
            move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(Step::proceed())
                }
            }
        };

        let handler2 = {
            let counter = counter2.clone();
            move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(Step::proceed())
                }
            }
        };

        let combined: Box<dyn EventHandle> = handler1.and(handler2);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();

        // Both handlers should have been called
        assert_eq!(*counter1.lock().unwrap(), 1);
        assert_eq!(*counter2.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_event_handle_ext_chain() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let handler1 = {
            let events = events.clone();
            move |event: LifecycleEvent, _conversation: &mut Conversation| {
                let events = events.clone();
                async move {
                    events.lock().unwrap().push(format!("h1:{:?}", event));
                    Ok(Step::proceed())
                }
            }
        };

        let handler2 = {
            let events = events.clone();
            move |event: LifecycleEvent, _conversation: &mut Conversation| {
                let events = events.clone();
                async move {
                    events.lock().unwrap().push(format!("h2:{:?}", event));
                    Ok(Step::proceed())
                }
            }
        };

        let handler3 = {
            let events = events.clone();
            move |event: LifecycleEvent, _conversation: &mut Conversation| {
                let events = events.clone();
                async move {
                    events.lock().unwrap().push(format!("h3:{:?}", event));
                    Ok(Step::proceed())
                }
            }
        };

        // Chain handlers using and()
        let combined: Box<dyn EventHandle> = handler1.and(handler2).and(handler3);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 3);
        assert!(handled[0].starts_with("h1:Start"));
        assert!(handled[1].starts_with("h2:Start"));
        assert!(handled[2].starts_with("h3:Start"));
    }

    #[tokio::test]
    async fn test_event_handle_ext_with_hook() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let start_title = std::sync::Arc::new(std::sync::Mutex::new(None));

        let start_handler = {
            let start_title = start_title.clone();
            move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                let start_title = start_title.clone();
                async move {
                    *start_title.lock().unwrap() = Some("Started".to_string());
                    Ok(Step::proceed())
                }
            }
        };

        let logging_handler = {
            let events = events.clone();
            move |event: LifecycleEvent, _conversation: &mut Conversation| {
                let events = events.clone();
                async move {
                    events.lock().unwrap().push(format!("Event: {:?}", event));
                    Ok(Step::proceed())
                }
            }
        };

        // Combine handlers using extension trait
        let combined_handler: Box<dyn EventHandle> = start_handler.and(logging_handler);

        let hook = Hook::default().on_start(combined_handler);

        let mut conversation = Conversation::generate();
        let _ = hook
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();

        assert_eq!(events.lock().unwrap().len(), 1);
        assert!(events.lock().unwrap()[0].starts_with("Event: Start"));
    }

    #[tokio::test]
    async fn test_hook_as_event_handle() {
        let start_title = std::sync::Arc::new(std::sync::Mutex::new(None));
        let end_title = std::sync::Arc::new(std::sync::Mutex::new(None));

        let hook = Hook::default()
            .on_start({
                let start_title = start_title.clone();
                move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                    let start_title = start_title.clone();
                    async move {
                        *start_title.lock().unwrap() = Some("Started".to_string());
                        Ok(Step::proceed())
                    }
                }
            })
            .on_end({
                let end_title = end_title.clone();
                move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                    let end_title = end_title.clone();
                    async move {
                        *end_title.lock().unwrap() = Some("Ended".to_string());
                        Ok(Step::proceed())
                    }
                }
            });

        // Test using handle() directly (EventHandle trait)
        let mut conversation = Conversation::generate();
        let step = hook
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();
        assert_eq!(*start_title.lock().unwrap(), Some("Started".to_string()));
        assert!(step.should_proceed());

        let step = hook
            .handle(LifecycleEvent::End, &mut conversation)
            .await
            .unwrap();
        assert_eq!(*end_title.lock().unwrap(), Some("Ended".to_string()));
        assert!(step.should_proceed());
    }

    #[tokio::test]
    async fn test_hook_combination_with_and() {
        let hook1_title = std::sync::Arc::new(std::sync::Mutex::new(None));
        let hook2_title = std::sync::Arc::new(std::sync::Mutex::new(None));

        let hook1 = Hook::default().on_start({
            let hook1_title = hook1_title.clone();
            move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                let hook1_title = hook1_title.clone();
                async move {
                    *hook1_title.lock().unwrap() = Some("Started".to_string());
                    Ok(Step::proceed())
                }
            }
        });
        let hook2 = Hook::default().on_start({
            let hook2_title = hook2_title.clone();
            move |_event: LifecycleEvent, _conversation: &mut Conversation| {
                let hook2_title = hook2_title.clone();
                async move {
                    *hook2_title.lock().unwrap() = Some("Ended".to_string());
                    Ok(Step::proceed())
                }
            }
        });

        // Combine hooks using and() extension method
        let combined: Box<dyn EventHandle> = hook1.and(hook2);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(
                LifecycleEvent::Start { agent: test_agent(), model_id: test_model_id() },
                &mut conversation,
            )
            .await
            .unwrap();

        // Both handlers should have been called
        assert_eq!(*hook1_title.lock().unwrap(), Some("Started".to_string()));
        assert_eq!(*hook2_title.lock().unwrap(), Some("Ended".to_string()));
    }
}
