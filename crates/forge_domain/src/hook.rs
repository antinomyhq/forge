use async_trait::async_trait;
use derive_setters::Setters;
use std::fmt;

use crate::{Conversation, InterruptionReason};

/// Lifecycle events that can occur during conversation processing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    /// Event fired when conversation processing starts
    Start,
    /// Event fired when conversation processing ends
    End,
    /// Event fired when a request is made to the LLM
    Request,
    /// Event fired when a response is received from the LLM
    Response,
    /// Event fired when a tool call starts
    ToolcallStart,
    /// Event fired when a tool call ends
    ToolcallEnd,
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

    /// Returns the reason if this is a Suspend step
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
/// This trait provides methods to combine multiple event handlers into a single handler
/// that executes them in sequence.
pub trait EventHandleExt: EventHandle {
    /// Combines this handler with another handler, creating a new handler that runs both in sequence
    ///
    /// When an event is handled, both handlers will be called in sequence.
    /// This handler runs first, then the other handler.
    /// The result from the other handler is returned.
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
        on_start: Box<dyn EventHandle>,
        on_end: Box<dyn EventHandle>,
        on_request: Box<dyn EventHandle>,
        on_response: Box<dyn EventHandle>,
        on_toolcall_start: Box<dyn EventHandle>,
        on_toolcall_end: Box<dyn EventHandle>,
    ) -> Self {
        Self {
            on_start,
            on_end,
            on_request,
            on_response,
            on_toolcall_start,
            on_toolcall_end,
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

    /// Combines this hook with another hook, creating a new hook that runs both handlers
    ///
    /// When an event is handled, both hooks' handlers will be called in sequence.
    /// The first hook's handler runs first, then the second hook's handler.
    /// The result from the second hook is returned.
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
            LifecycleEvent::Start => self.on_start.handle(event, conversation).await,
            LifecycleEvent::End => self.on_end.handle(event, conversation).await,
            LifecycleEvent::Request => self.on_request.handle(event, conversation).await,
            LifecycleEvent::Response => self.on_response.handle(event, conversation).await,
            LifecycleEvent::ToolcallStart => {
                self.on_toolcall_start.handle(event, conversation).await
            }
            LifecycleEvent::ToolcallEnd => self.on_toolcall_end.handle(event, conversation).await,
        }
    }
}

/// A handler that combines two event handlers, running both in sequence
///
/// This is used internally by the `Hook::zip` method to combine two hooks.
struct CombinedHandler(Box<dyn EventHandle>, Box<dyn EventHandle>);

#[async_trait]
impl EventHandle for CombinedHandler {
    async fn handle(
        &self,
        event: LifecycleEvent,
        conversation: &mut Conversation,
    ) -> anyhow::Result<Step> {
        // Run the first handler
        let step = self.0.handle(event, conversation).await?;
        match step {
            Step::Proceed => {
                // Run the second handler and return its result
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[derive(Debug, Default)]
    struct TestHandler {
        events_handled: std::sync::Arc<std::sync::Mutex<Vec<LifecycleEvent>>>,
    }

    #[async_trait]
    impl EventHandle for TestHandler {
        async fn handle(
            &self,
            event: LifecycleEvent,
            _conversation: &mut Conversation,
        ) -> anyhow::Result<Step> {
            self.events_handled.lock().unwrap().push(event);
            Ok(Step::proceed())
        }
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
        let handler = TestHandler { events_handled: events.clone() };

        let hook = Hook::default().on_start(Box::new(handler));
        let mut conversation = Conversation::generate();

        let step = hook
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();
        assert!(step.should_proceed());

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 1);
        assert_eq!(handled[0], LifecycleEvent::Start);
    }

    #[tokio::test]
    async fn test_hook_builder() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let hook = Hook::default()
            .on_start(Box::new(TestHandler { events_handled: events.clone() }))
            .on_end(Box::new(TestHandler { events_handled: events.clone() }))
            .on_request(Box::new(TestHandler { events_handled: events.clone() }));

        let mut conversation = Conversation::generate();

        // Test Start event
        let _ = hook
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();
        // Test End event
        let _ = hook
            .handle(LifecycleEvent::End, &mut conversation)
            .await
            .unwrap();
        // Test Request event
        let _ = hook
            .handle(LifecycleEvent::Request, &mut conversation)
            .await
            .unwrap();

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 3);
        assert_eq!(handled[0], LifecycleEvent::Start);
        assert_eq!(handled[1], LifecycleEvent::End);
        assert_eq!(handled[2], LifecycleEvent::Request);
    }

    #[tokio::test]
    async fn test_hook_all_events() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let hook = Hook::new(
            Box::new(TestHandler { events_handled: events.clone() }),
            Box::new(TestHandler { events_handled: events.clone() }),
            Box::new(TestHandler { events_handled: events.clone() }),
            Box::new(TestHandler { events_handled: events.clone() }),
            Box::new(TestHandler { events_handled: events.clone() }),
            Box::new(TestHandler { events_handled: events.clone() }),
        );

        let mut conversation = Conversation::generate();

        let all_events = vec![
            LifecycleEvent::Start,
            LifecycleEvent::End,
            LifecycleEvent::Request,
            LifecycleEvent::Response,
            LifecycleEvent::ToolcallStart,
            LifecycleEvent::ToolcallEnd,
        ];

        for event in all_events {
            let _ = hook.handle(event, &mut conversation).await.unwrap();
        }

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 6);
    }

    #[tokio::test]
    async fn test_step_mutable_conversation() {
        struct MutableHandler;

        #[async_trait]
        impl EventHandle for MutableHandler {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                // Modify the conversation
                conversation.title = Some("Modified title".to_string());
                Ok(Step::proceed())
            }
        }

        let hook = Hook::default().on_start(Box::new(MutableHandler));
        let mut conversation = Conversation::generate();

        assert!(conversation.title.is_none());

        let step = hook
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();

        assert!(step.should_proceed());
        assert_eq!(conversation.title, Some("Modified title".to_string()));
    }

    #[tokio::test]
    async fn test_step_halt_variant() {
        struct HaltHandler;

        #[async_trait]
        impl EventHandle for HaltHandler {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                _conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                Ok(Step::interrupt(InterruptionReason::MaxRequestPerTurnLimitReached {
                    limit: 5,
                }))
            }
        }

        let hook = Hook::default().on_start(Box::new(HaltHandler));
        let mut conversation = Conversation::generate();

        let step = hook
            .handle(LifecycleEvent::Start, &mut conversation)
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
        struct Handler1 {
            counter: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        #[async_trait]
        impl EventHandle for Handler1 {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                _conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                *self.counter.lock().unwrap() += 1;
                Ok(Step::proceed())
            }
        }

        struct Handler2 {
            counter: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        #[async_trait]
        impl EventHandle for Handler2 {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                _conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                *self.counter.lock().unwrap() += 1;
                Ok(Step::proceed())
            }
        }

        let counter1 = std::sync::Arc::new(std::sync::Mutex::new(0));
        let counter2 = std::sync::Arc::new(std::sync::Mutex::new(0));

        let hook1 = Hook::default().on_start(Box::new(Handler1 { counter: counter1.clone() }));

        let hook2 = Hook::default().on_start(Box::new(Handler2 { counter: counter2.clone() }));

        let combined = hook1.zip(hook2);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();

        // Both handlers should have been called
        assert_eq!(*counter1.lock().unwrap(), 1);
        assert_eq!(*counter2.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_hook_zip_multiple() {
        struct Handler {
            id: &'static str,
            events: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        }

        #[async_trait]
        impl EventHandle for Handler {
            async fn handle(
                &self,
                event: LifecycleEvent,
                _conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                self.events
                    .lock()
                    .unwrap()
                    .push(format!("{}:{:?}", self.id, event));
                Ok(Step::proceed())
            }
        }

        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let hook1 = Hook::default().on_start(Box::new(Handler { id: "h1", events: events.clone() }));

        let hook2 = Hook::default().on_start(Box::new(Handler { id: "h2", events: events.clone() }));

        let hook3 = Hook::default().on_start(Box::new(Handler { id: "h3", events: events.clone() }));

        let combined = hook1.zip(hook2).zip(hook3);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 3);
        assert_eq!(handled[0], "h1:Start");
        assert_eq!(handled[1], "h2:Start");
        assert_eq!(handled[2], "h3:Start");
    }

    #[tokio::test]
    async fn test_hook_zip_different_events() {
        struct StartHandler;
        struct EndHandler;

        #[async_trait]
        impl EventHandle for StartHandler {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                conversation.title = Some("Start".to_string());
                Ok(Step::proceed())
            }
        }

        #[async_trait]
        impl EventHandle for EndHandler {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                conversation.title = Some("End".to_string());
                Ok(Step::proceed())
            }
        }

        let hook1 = Hook::default()
            .on_start(Box::new(StartHandler))
            .on_end(Box::new(EndHandler));
        let hook2 = Hook::default();

        let combined = hook1.zip(hook2);

        let mut conversation = Conversation::generate();

        // Test Start event
        let _ = combined
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();
        assert_eq!(conversation.title, Some("Start".to_string()));

        // Test End event
        let _ = combined
            .handle(LifecycleEvent::End, &mut conversation)
            .await
            .unwrap();
        assert_eq!(conversation.title, Some("End".to_string()));
    }

    #[tokio::test]
    async fn test_event_handle_ext_and() {
        struct Handler1 {
            counter: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        #[async_trait]
        impl EventHandle for Handler1 {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                _conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                *self.counter.lock().unwrap() += 1;
                Ok(Step::proceed())
            }
        }

        struct Handler2 {
            counter: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        #[async_trait]
        impl EventHandle for Handler2 {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                _conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                *self.counter.lock().unwrap() += 1;
                Ok(Step::proceed())
            }
        }

        let counter1 = std::sync::Arc::new(std::sync::Mutex::new(0));
        let counter2 = std::sync::Arc::new(std::sync::Mutex::new(0));

        let handler1 = Handler1 { counter: counter1.clone() };
        let handler2 = Handler2 { counter: counter2.clone() };

        let combined = handler1.and(handler2);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();

        // Both handlers should have been called
        assert_eq!(*counter1.lock().unwrap(), 1);
        assert_eq!(*counter2.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_event_handle_ext_and_boxed() {
        struct Handler1 {
            counter: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        #[async_trait]
        impl EventHandle for Handler1 {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                _conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                *self.counter.lock().unwrap() += 1;
                Ok(Step::proceed())
            }
        }

        struct Handler2 {
            counter: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        #[async_trait]
        impl EventHandle for Handler2 {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                _conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                *self.counter.lock().unwrap() += 1;
                Ok(Step::proceed())
            }
        }

        let counter1 = std::sync::Arc::new(std::sync::Mutex::new(0));
        let counter2 = std::sync::Arc::new(std::sync::Mutex::new(0));

        let handler1 = Handler1 { counter: counter1.clone() };
        let handler2 = Box::new(Handler2 { counter: counter2.clone() });

        let combined = handler1.and(*handler2);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();

        // Both handlers should have been called
        assert_eq!(*counter1.lock().unwrap(), 1);
        assert_eq!(*counter2.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_event_handle_ext_chain() {
        struct Handler {
            id: &'static str,
            events: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        }

        #[async_trait]
        impl EventHandle for Handler {
            async fn handle(
                &self,
                event: LifecycleEvent,
                _conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                self.events
                    .lock()
                    .unwrap()
                    .push(format!("{}:{:?}", self.id, event));
                Ok(Step::proceed())
            }
        }

        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let handler1 = Handler { id: "h1", events: events.clone() };

        let handler2 = Handler { id: "h2", events: events.clone() };

        let handler3 = Handler { id: "h3", events: events.clone() };

        // Chain handlers using and()
        let combined = handler1.and(handler2).and(handler3);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 3);
        assert_eq!(handled[0], "h1:Start");
        assert_eq!(handled[1], "h2:Start");
        assert_eq!(handled[2], "h3:Start");
    }

    #[tokio::test]
    async fn test_event_handle_ext_with_hook() {
        struct StartHandler;
        struct LoggingHandler {
            events: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        }

        #[async_trait]
        impl EventHandle for StartHandler {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                conversation.title = Some("Started".to_string());
                Ok(Step::proceed())
            }
        }

        #[async_trait]
        impl EventHandle for LoggingHandler {
            async fn handle(
                &self,
                event: LifecycleEvent,
                _conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                self.events
                    .lock()
                    .unwrap()
                    .push(format!("Event: {:?}", event));
                Ok(Step::proceed())
            }
        }

        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        // Combine handlers using extension trait
        let combined_handler = StartHandler.and(LoggingHandler { events: events.clone() });

        let hook = Hook::default().on_start(combined_handler);

        let mut conversation = Conversation::generate();
        let _ = hook
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();

        assert_eq!(conversation.title, Some("Started".to_string()));
        assert_eq!(events.lock().unwrap().len(), 1);
        assert_eq!(events.lock().unwrap()[0], "Event: Start");
    }

    #[tokio::test]
    async fn test_hook_as_event_handle() {
        struct StartHandler;
        struct EndHandler;

        #[async_trait]
        impl EventHandle for StartHandler {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                conversation.title = Some("Started".to_string());
                Ok(Step::proceed())
            }
        }

        #[async_trait]
        impl EventHandle for EndHandler {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                conversation.title = Some("Ended".to_string());
                Ok(Step::proceed())
            }
        }

        let hook = Hook::default()
            .on_start(Box::new(StartHandler))
            .on_end(Box::new(EndHandler));

        // Test using handle() directly (EventHandle trait)
        let mut conversation = Conversation::generate();
        let step = hook
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();
        assert_eq!(conversation.title, Some("Started".to_string()));
        assert!(step.should_proceed());

        let step = hook
            .handle(LifecycleEvent::End, &mut conversation)
            .await
            .unwrap();
        assert_eq!(conversation.title, Some("Ended".to_string()));
        assert!(step.should_proceed());
    }

    #[tokio::test]
    async fn test_hook_combination_with_and() {
        struct StartHandler;
        struct EndHandler;

        #[async_trait]
        impl EventHandle for StartHandler {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                conversation.title = Some("Started".to_string());
                Ok(Step::proceed())
            }
        }

        #[async_trait]
        impl EventHandle for EndHandler {
            async fn handle(
                &self,
                _event: LifecycleEvent,
                conversation: &mut Conversation,
            ) -> anyhow::Result<Step> {
                conversation.title = Some("Ended".to_string());
                Ok(Step::proceed())
            }
        }

        let hook1 = Hook::default().on_start(Box::new(StartHandler));
        let hook2 = Hook::default().on_start(Box::new(EndHandler));

        // Combine hooks using and() extension method
        let combined = hook1.and(hook2);

        let mut conversation = Conversation::generate();
        let _ = combined
            .handle(LifecycleEvent::Start, &mut conversation)
            .await
            .unwrap();

        // Both handlers should have been called
        // The last handler's result determines the final title
        assert_eq!(conversation.title, Some("Ended".to_string()));
    }
}
