use std::path::PathBuf;

use async_trait::async_trait;
use derive_more::From;
use derive_setters::Setters;

use crate::{
    Agent, ChatCompletionMessageFull, ConfigChangePayload, Conversation, CwdChangedPayload,
    ElicitationPayload, ElicitationResultPayload, FileChangedPayload, InstructionsLoadedPayload,
    ModelId, NotificationPayload, PermissionDeniedPayload, PermissionRequestPayload,
    PostCompactPayload, PostToolUseFailurePayload, PostToolUsePayload, PreCompactPayload,
    PreToolUsePayload, SessionEndPayload, SessionStartPayload, SetupPayload, StopFailurePayload,
    StopPayload, SubagentStartPayload, SubagentStopPayload, ToolCallFull, ToolResult,
    UserPromptSubmitPayload, WorktreeCreatePayload, WorktreeRemovePayload,
};

/// Sentinel session id attached to legacy [`EventData::new`] callers that
/// pre-date the plugin-hook context fields. [`EventData::with_context`]
/// replaces these sentinels with real session ids sourced from the
/// orchestrator.
pub const LEGACY_SESSION_ID: &str = "legacy";

/// Sentinel transcript path used by the legacy [`EventData::new`] ctor.
///
/// Kept as a `&'static str` so the constructor can build a `PathBuf` on
/// demand without requiring a const fn over `PathBuf`.
pub const LEGACY_TRANSCRIPT_PATH: &str = "/tmp/forge-legacy-transcript";

/// A container for lifecycle events with agent, model, and plugin-hook
/// context.
///
/// This struct provides a consistent structure for all lifecycle events,
/// containing the agent, model ID, and the base fields every Claude Code
/// plugin hook expects (`session_id`, `transcript_path`, `cwd`,
/// `permission_mode`) along with the event-specific payload data.
///
/// The legacy constructor [`EventData::new`] keeps existing call sites
/// working by filling the new fields with sentinel values;
/// [`EventData::with_context`] accepts the real values.
#[derive(Debug, PartialEq, Clone)]
pub struct EventData<P: Send + Sync> {
    /// The agent associated with this event
    pub agent: Agent,
    /// The model ID being used
    pub model_id: ModelId,
    /// Current session ID. Legacy callers get
    /// [`LEGACY_SESSION_ID`]; context-aware firing sites pass the real id.
    pub session_id: String,
    /// Absolute path to the transcript file for this session.
    pub transcript_path: PathBuf,
    /// Current working directory at the time the event fired.
    pub cwd: PathBuf,
    /// Optional permission mode (`"default"`, `"acceptEdits"`, ...).
    pub permission_mode: Option<String>,
    /// Event-specific payload data
    pub payload: P,
}

impl<P: Send + Sync> EventData<P> {
    /// Creates a new event with the given agent, model ID, and payload.
    ///
    /// **Legacy constructor** — kept as a thin wrapper so call sites that
    /// do not supply plugin-hook context still compile. The base fields
    /// are filled with sentinels:
    ///
    /// - `session_id` → [`LEGACY_SESSION_ID`]
    /// - `transcript_path` → [`LEGACY_TRANSCRIPT_PATH`]
    /// - `cwd` → `std::env::current_dir()` or the empty path on error
    /// - `permission_mode` → `None`
    ///
    /// Prefer [`EventData::with_context`] for new code, which accepts
    /// proper values sourced from the orchestrator.
    pub fn new(agent: Agent, model_id: ModelId, payload: P) -> Self {
        Self {
            agent,
            model_id,
            session_id: LEGACY_SESSION_ID.to_string(),
            transcript_path: PathBuf::from(LEGACY_TRANSCRIPT_PATH),
            cwd: std::env::current_dir().unwrap_or_default(),
            permission_mode: None,
            payload,
        }
    }

    /// Creates a new event with fully-populated plugin-hook context.
    ///
    /// Used by firing sites that know the real session id,
    /// transcript path, cwd, and (optional) permission mode.
    pub fn with_context(
        agent: Agent,
        model_id: ModelId,
        session_id: impl Into<String>,
        transcript_path: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        payload: P,
    ) -> Self {
        Self {
            agent,
            model_id,
            session_id: session_id.into(),
            transcript_path: transcript_path.into(),
            cwd: cwd.into(),
            permission_mode: None,
            payload,
        }
    }

    /// Attach a permission mode to an already-built `EventData`.
    pub fn with_permission_mode(mut self, mode: impl Into<String>) -> Self {
        self.permission_mode = Some(mode.into());
        self
    }
}

/// Payload for the Start event
#[derive(Debug, PartialEq, Clone, Default)]
pub struct StartPayload;

/// Payload for the End event
#[derive(Debug, PartialEq, Clone, Default)]
pub struct EndPayload;

/// Payload for the Request event
#[derive(Debug, PartialEq, Clone, Setters)]
#[setters(into)]
pub struct RequestPayload {
    /// The number of requests made
    pub request_count: usize,
}

impl RequestPayload {
    /// Creates a new request payload
    pub fn new(request_count: usize) -> Self {
        Self { request_count }
    }
}

/// Payload for the Response event
#[derive(Debug, PartialEq, Clone, Setters)]
#[setters(into)]
pub struct ResponsePayload {
    /// The full response message from the LLM
    pub message: ChatCompletionMessageFull,
}

impl ResponsePayload {
    /// Creates a new response payload
    pub fn new(message: ChatCompletionMessageFull) -> Self {
        Self { message }
    }
}

/// Payload for the ToolcallStart event
#[derive(Debug, PartialEq, Clone, Setters)]
#[setters(into)]
pub struct ToolcallStartPayload {
    /// The tool call details
    pub tool_call: ToolCallFull,
}

impl ToolcallStartPayload {
    /// Creates a new tool call start payload
    pub fn new(tool_call: ToolCallFull) -> Self {
        Self { tool_call }
    }
}

/// Payload for the ToolcallEnd event
#[derive(Debug, PartialEq, Clone, Setters)]
#[setters(into)]
pub struct ToolcallEndPayload {
    /// The original tool call that was executed
    pub tool_call: ToolCallFull,
    /// The tool result (success or failure)
    pub result: ToolResult,
}

impl ToolcallEndPayload {
    /// Creates a new tool call end payload
    pub fn new(tool_call: ToolCallFull, result: ToolResult) -> Self {
        Self { tool_call, result }
    }
}

/// Lifecycle events that can occur during conversation processing.
///
/// The first block of variants is the legacy set — they drive Forge's
/// internal handlers (tracing, title generation, etc.). The second block
/// is the Claude-Code plugin-hook set: these variants map 1-to-1 with the
/// hook slots on [`Hook`] and are fired by the orchestrator.
///
/// Marked `#[non_exhaustive]` so downstream consumers are nudged into
/// matching with a wildcard arm — new variants may be added in the future.
#[derive(Debug, PartialEq, Clone, From)]
#[non_exhaustive]
pub enum LifecycleEvent {
    // ---- Legacy ----
    /// INTERNAL: Used by tracing and title generation only. External
    /// plugins should use `SessionStart`.
    #[doc(hidden)]
    #[deprecated(since = "0.1.0", note = "use SessionStart instead")]
    Start(EventData<StartPayload>),

    /// INTERNAL: Used by tracing and title generation only. External
    /// plugins should use `SessionEnd` or `Stop`.
    #[doc(hidden)]
    #[deprecated(since = "0.1.0", note = "use SessionEnd instead")]
    End(EventData<EndPayload>),

    /// INTERNAL: Used by doom-loop detection and skill listing. No
    /// external plugin equivalent.
    #[doc(hidden)]
    #[deprecated(since = "0.1.0", note = "use UserPromptSubmit instead")]
    Request(EventData<RequestPayload>),

    /// INTERNAL: Used by tracing and compaction trigger. External
    /// plugins should use `PreCompact`/`PostCompact`.
    #[doc(hidden)]
    #[deprecated(since = "0.1.0", note = "use PostToolUse/PostToolUseFailure instead")]
    Response(EventData<ResponsePayload>),

    /// INTERNAL: Used by tracing only. External plugins should use
    /// `PreToolUse`.
    #[doc(hidden)]
    #[deprecated(since = "0.1.0", note = "use PreToolUse instead")]
    ToolcallStart(EventData<ToolcallStartPayload>),

    /// INTERNAL: Used by tracing only. External plugins should use
    /// `PostToolUse`/`PostToolUseFailure`.
    #[doc(hidden)]
    #[deprecated(since = "0.1.0", note = "use PostToolUse instead")]
    ToolcallEnd(EventData<ToolcallEndPayload>),

    // ---- Claude Code plugin-hook events ----
    /// Fired before a tool call executes. Hooks can approve, deny, or
    /// rewrite the tool input.
    PreToolUse(EventData<PreToolUsePayload>),

    /// Fired after a tool call completes successfully.
    PostToolUse(EventData<PostToolUsePayload>),

    /// Fired after a tool call errors out (including user interrupts).
    PostToolUseFailure(EventData<PostToolUseFailurePayload>),

    /// Fired when the user submits a new prompt.
    UserPromptSubmit(EventData<UserPromptSubmitPayload>),

    /// Fired at the start of a session (startup / resume / clear / compact).
    SessionStart(EventData<SessionStartPayload>),

    /// Fired when a session ends (clear / logout / exit / ...).
    SessionEnd(EventData<SessionEndPayload>),

    /// Fired when the agent loop finishes a turn naturally.
    Stop(EventData<StopPayload>),

    /// Fired when the agent loop halts due to an error.
    StopFailure(EventData<StopFailurePayload>),

    /// Fired just before a compaction cycle starts.
    PreCompact(EventData<PreCompactPayload>),

    /// Fired after a compaction cycle finishes.
    PostCompact(EventData<PostCompactPayload>),

    // ---- Notification / Setup / Config plugin-hook events ----
    /// Fired when Forge wants to surface a user-facing notification
    /// (idle prompt, OAuth success, elicitation update, …).
    Notification(EventData<NotificationPayload>),

    /// Fired once per `forge --init` / `forge --maintenance` invocation.
    Setup(EventData<SetupPayload>),

    /// Fired when a configuration file watched by `ConfigWatcher`
    /// changes on disk (debounced, with internal-write suppression).
    /// The hook slot is wired; the `ConfigWatcher` fire loop that
    /// actually raises this event is not yet implemented.
    ConfigChange(EventData<ConfigChangePayload>),

    /// Fired whenever Forge loads an instructions / memory file
    /// (`AGENTS.md` etc). The hook slot is wired; fire sites inside
    /// `CustomInstructionsService` are pending.
    InstructionsLoaded(EventData<InstructionsLoadedPayload>),

    // ---- Subagent / Permission / File / Worktree plugin-hook events ----
    /// Fired when a sub-agent starts running inside the orchestrator.
    /// The hook slot is wired; fire sites in `agent_executor.rs` are
    /// pending until `agent_id` is threaded through the orchestrator.
    SubagentStart(EventData<SubagentStartPayload>),

    /// Fired when a sub-agent finishes its turn.
    SubagentStop(EventData<SubagentStopPayload>),

    /// Fired when a tool call needs permission that hasn't been granted
    /// yet. The hook slot is wired; the fire site in `policy.rs` is
    /// pending.
    PermissionRequest(EventData<PermissionRequestPayload>),

    /// Fired when a permission request is rejected.
    PermissionDenied(EventData<PermissionDeniedPayload>),

    /// Fired when the orchestrator's current working directory changes.
    /// The hook slot is wired; the fire site in the Shell tool / cwd
    /// tracker is pending.
    CwdChanged(EventData<CwdChangedPayload>),

    /// Fired when a tracked file is added, modified, or removed.
    /// The hook slot is wired; the `FileChangedWatcher` service is
    /// pending.
    FileChanged(EventData<FileChangedPayload>),

    /// Fired when the agent enters a new git worktree via
    /// `EnterWorktreeTool` or a hook-driven VCS adapter. The hook slot
    /// is wired; the worktree tools and sandbox fire sites are pending.
    WorktreeCreate(EventData<WorktreeCreatePayload>),

    /// Fired when the agent exits a git worktree via
    /// `ExitWorktreeTool` or a hook-driven VCS adapter. The hook slot
    /// is wired; fire sites are pending.
    WorktreeRemove(EventData<WorktreeRemovePayload>),

    // ---- MCP elicitation hooks ----
    /// Fired by the MCP client before it prompts the user for
    /// additional input on behalf of an MCP server. The hook slot is
    /// wired; the MCP client integration that emits this event is
    /// pending.
    Elicitation(EventData<ElicitationPayload>),

    /// Fired after the user (or an auto-responding plugin hook)
    /// completes the elicitation.
    ElicitationResult(EventData<ElicitationResultPayload>),
}

/// Trait for handling lifecycle events
///
/// Implementations of this trait can be used to react to different
/// stages of conversation processing.
#[async_trait]
pub trait EventHandle<T: Send + Sync>: Send + Sync {
    /// Handles a lifecycle event and potentially modifies the conversation
    ///
    /// # Arguments
    /// * `event` - The lifecycle event that occurred
    /// * `conversation` - The current conversation state (mutable)
    ///
    /// # Errors
    /// Returns an error if the event handling fails
    async fn handle(&self, event: &T, conversation: &mut Conversation) -> anyhow::Result<()>;
}

/// Extension trait for combining event handlers
///
/// This trait provides methods to combine multiple event handlers into a single
/// handler that executes them in sequence.
pub trait EventHandleExt<T: Send + Sync>: EventHandle<T> {
    /// Combines this handler with another handler, creating a new handler that
    /// runs both in sequence
    ///
    /// When an event is handled, both handlers run in sequence.
    ///
    /// # Arguments
    /// * `other` - Another handler to combine with this one
    ///
    /// # Returns
    /// A new boxed handler that combines both handlers
    fn and<H: EventHandle<T> + 'static>(self, other: H) -> Box<dyn EventHandle<T>>
    where
        Self: Sized + 'static;
}

impl<T: Send + Sync + 'static, A: EventHandle<T> + 'static> EventHandleExt<T> for A {
    fn and<H: EventHandle<T> + 'static>(self, other: H) -> Box<dyn EventHandle<T>>
    where
        Self: Sized + 'static,
    {
        Box::new(CombinedHandler(Box::new(self), Box::new(other)))
    }
}

// Implement EventHandle for Box<dyn EventHandle> to allow using boxed handlers
#[async_trait]
impl<T: Send + Sync> EventHandle<T> for Box<dyn EventHandle<T>> {
    async fn handle(&self, event: &T, conversation: &mut Conversation) -> anyhow::Result<()> {
        (**self).handle(event, conversation).await
    }
}

/// A hook that contains handlers for all lifecycle events
///
/// Hooks allow you to attach custom behavior at specific points
/// during conversation processing.
pub struct Hook {
    // ---- Legacy slots ----
    on_start: Box<dyn EventHandle<EventData<StartPayload>>>,
    on_end: Box<dyn EventHandle<EventData<EndPayload>>>,
    on_request: Box<dyn EventHandle<EventData<RequestPayload>>>,
    on_response: Box<dyn EventHandle<EventData<ResponsePayload>>>,
    on_toolcall_start: Box<dyn EventHandle<EventData<ToolcallStartPayload>>>,
    on_toolcall_end: Box<dyn EventHandle<EventData<ToolcallEndPayload>>>,

    // ---- Claude Code plugin-hook slots ----
    on_pre_tool_use: Box<dyn EventHandle<EventData<PreToolUsePayload>>>,
    on_post_tool_use: Box<dyn EventHandle<EventData<PostToolUsePayload>>>,
    on_post_tool_use_failure: Box<dyn EventHandle<EventData<PostToolUseFailurePayload>>>,
    on_user_prompt_submit: Box<dyn EventHandle<EventData<UserPromptSubmitPayload>>>,
    on_session_start: Box<dyn EventHandle<EventData<SessionStartPayload>>>,
    on_session_end: Box<dyn EventHandle<EventData<SessionEndPayload>>>,
    on_stop: Box<dyn EventHandle<EventData<StopPayload>>>,
    on_stop_failure: Box<dyn EventHandle<EventData<StopFailurePayload>>>,
    on_pre_compact: Box<dyn EventHandle<EventData<PreCompactPayload>>>,
    on_post_compact: Box<dyn EventHandle<EventData<PostCompactPayload>>>,

    // ---- Notification / Setup / Config slots ----
    on_notification: Box<dyn EventHandle<EventData<NotificationPayload>>>,
    on_setup: Box<dyn EventHandle<EventData<SetupPayload>>>,
    on_config_change: Box<dyn EventHandle<EventData<ConfigChangePayload>>>,
    on_instructions_loaded: Box<dyn EventHandle<EventData<InstructionsLoadedPayload>>>,

    // ---- Subagent / Permission / File / Worktree slots ----
    on_subagent_start: Box<dyn EventHandle<EventData<SubagentStartPayload>>>,
    on_subagent_stop: Box<dyn EventHandle<EventData<SubagentStopPayload>>>,
    on_permission_request: Box<dyn EventHandle<EventData<PermissionRequestPayload>>>,
    on_permission_denied: Box<dyn EventHandle<EventData<PermissionDeniedPayload>>>,
    on_cwd_changed: Box<dyn EventHandle<EventData<CwdChangedPayload>>>,
    on_file_changed: Box<dyn EventHandle<EventData<FileChangedPayload>>>,
    on_worktree_create: Box<dyn EventHandle<EventData<WorktreeCreatePayload>>>,
    on_worktree_remove: Box<dyn EventHandle<EventData<WorktreeRemovePayload>>>,

    // ---- MCP elicitation slots ----
    on_elicitation: Box<dyn EventHandle<EventData<ElicitationPayload>>>,
    on_elicitation_result: Box<dyn EventHandle<EventData<ElicitationResultPayload>>>,
}

impl Default for Hook {
    fn default() -> Self {
        Self {
            on_start: Box::new(NoOpHandler),
            on_end: Box::new(NoOpHandler),
            on_request: Box::new(NoOpHandler),
            on_response: Box::new(NoOpHandler),
            on_toolcall_start: Box::new(NoOpHandler),
            on_toolcall_end: Box::new(NoOpHandler),
            on_pre_tool_use: Box::new(NoOpHandler),
            on_post_tool_use: Box::new(NoOpHandler),
            on_post_tool_use_failure: Box::new(NoOpHandler),
            on_user_prompt_submit: Box::new(NoOpHandler),
            on_session_start: Box::new(NoOpHandler),
            on_session_end: Box::new(NoOpHandler),
            on_stop: Box::new(NoOpHandler),
            on_stop_failure: Box::new(NoOpHandler),
            on_pre_compact: Box::new(NoOpHandler),
            on_post_compact: Box::new(NoOpHandler),
            on_notification: Box::new(NoOpHandler),
            on_setup: Box::new(NoOpHandler),
            on_config_change: Box::new(NoOpHandler),
            on_instructions_loaded: Box::new(NoOpHandler),
            on_subagent_start: Box::new(NoOpHandler),
            on_subagent_stop: Box::new(NoOpHandler),
            on_permission_request: Box::new(NoOpHandler),
            on_permission_denied: Box::new(NoOpHandler),
            on_cwd_changed: Box::new(NoOpHandler),
            on_file_changed: Box::new(NoOpHandler),
            on_worktree_create: Box::new(NoOpHandler),
            on_worktree_remove: Box::new(NoOpHandler),
            on_elicitation: Box::new(NoOpHandler),
            on_elicitation_result: Box::new(NoOpHandler),
        }
    }
}

impl Hook {
    /// Creates a new hook with custom handlers for all event types
    ///
    /// # Arguments
    /// * `on_start` - Handler for start events
    /// * `on_end` - Handler for end events
    /// * `on_request` - Handler for request events
    /// * `on_response` - Handler for response events
    /// * `on_toolcall_start` - Handler for tool call start events
    /// * `on_toolcall_end` - Handler for tool call end events
    pub fn new(
        on_start: impl Into<Box<dyn EventHandle<EventData<StartPayload>>>>,
        on_end: impl Into<Box<dyn EventHandle<EventData<EndPayload>>>>,
        on_request: impl Into<Box<dyn EventHandle<EventData<RequestPayload>>>>,
        on_response: impl Into<Box<dyn EventHandle<EventData<ResponsePayload>>>>,
        on_toolcall_start: impl Into<Box<dyn EventHandle<EventData<ToolcallStartPayload>>>>,
        on_toolcall_end: impl Into<Box<dyn EventHandle<EventData<ToolcallEndPayload>>>>,
    ) -> Self {
        // Only the legacy slots are customizable via `new()`; plugin-hook
        // slots default to `NoOpHandler` and are attached via the builder
        // methods (`on_pre_tool_use`, ...).
        Self {
            on_start: on_start.into(),
            on_end: on_end.into(),
            on_request: on_request.into(),
            on_response: on_response.into(),
            on_toolcall_start: on_toolcall_start.into(),
            on_toolcall_end: on_toolcall_end.into(),
            on_pre_tool_use: Box::new(NoOpHandler),
            on_post_tool_use: Box::new(NoOpHandler),
            on_post_tool_use_failure: Box::new(NoOpHandler),
            on_user_prompt_submit: Box::new(NoOpHandler),
            on_session_start: Box::new(NoOpHandler),
            on_session_end: Box::new(NoOpHandler),
            on_stop: Box::new(NoOpHandler),
            on_stop_failure: Box::new(NoOpHandler),
            on_pre_compact: Box::new(NoOpHandler),
            on_post_compact: Box::new(NoOpHandler),
            on_notification: Box::new(NoOpHandler),
            on_setup: Box::new(NoOpHandler),
            on_config_change: Box::new(NoOpHandler),
            on_instructions_loaded: Box::new(NoOpHandler),
            on_subagent_start: Box::new(NoOpHandler),
            on_subagent_stop: Box::new(NoOpHandler),
            on_permission_request: Box::new(NoOpHandler),
            on_permission_denied: Box::new(NoOpHandler),
            on_cwd_changed: Box::new(NoOpHandler),
            on_file_changed: Box::new(NoOpHandler),
            on_worktree_create: Box::new(NoOpHandler),
            on_worktree_remove: Box::new(NoOpHandler),
            on_elicitation: Box::new(NoOpHandler),
            on_elicitation_result: Box::new(NoOpHandler),
        }
    }
}

impl Hook {
    /// Sets the start event handler
    ///
    /// # Arguments
    /// * `handler` - Handler for start events (automatically boxed)
    pub fn on_start(
        mut self,
        handler: impl EventHandle<EventData<StartPayload>> + 'static,
    ) -> Self {
        self.on_start = Box::new(handler);
        self
    }

    /// Sets the end event handler
    ///
    /// # Arguments
    /// * `handler` - Handler for end events (automatically boxed)
    pub fn on_end(mut self, handler: impl EventHandle<EventData<EndPayload>> + 'static) -> Self {
        self.on_end = Box::new(handler);
        self
    }

    /// Sets the request event handler
    ///
    /// # Arguments
    /// * `handler` - Handler for request events (automatically boxed)
    pub fn on_request(
        mut self,
        handler: impl EventHandle<EventData<RequestPayload>> + 'static,
    ) -> Self {
        self.on_request = Box::new(handler);
        self
    }

    /// Sets the response event handler
    ///
    /// # Arguments
    /// * `handler` - Handler for response events (automatically boxed)
    pub fn on_response(
        mut self,
        handler: impl EventHandle<EventData<ResponsePayload>> + 'static,
    ) -> Self {
        self.on_response = Box::new(handler);
        self
    }

    /// Sets the tool call start event handler
    ///
    /// # Arguments
    /// * `handler` - Handler for tool call start events (automatically boxed)
    pub fn on_toolcall_start(
        mut self,
        handler: impl EventHandle<EventData<ToolcallStartPayload>> + 'static,
    ) -> Self {
        self.on_toolcall_start = Box::new(handler);
        self
    }

    /// Sets the tool call end event handler
    ///
    /// # Arguments
    /// * `handler` - Handler for tool call end events (automatically boxed)
    pub fn on_toolcall_end(
        mut self,
        handler: impl EventHandle<EventData<ToolcallEndPayload>> + 'static,
    ) -> Self {
        self.on_toolcall_end = Box::new(handler);
        self
    }

    // ---- Claude Code plugin-hook builder methods ----

    /// Sets the PreToolUse event handler.
    pub fn on_pre_tool_use(
        mut self,
        handler: impl EventHandle<EventData<PreToolUsePayload>> + 'static,
    ) -> Self {
        self.on_pre_tool_use = Box::new(handler);
        self
    }

    /// Sets the PostToolUse event handler.
    pub fn on_post_tool_use(
        mut self,
        handler: impl EventHandle<EventData<PostToolUsePayload>> + 'static,
    ) -> Self {
        self.on_post_tool_use = Box::new(handler);
        self
    }

    /// Sets the PostToolUseFailure event handler.
    pub fn on_post_tool_use_failure(
        mut self,
        handler: impl EventHandle<EventData<PostToolUseFailurePayload>> + 'static,
    ) -> Self {
        self.on_post_tool_use_failure = Box::new(handler);
        self
    }

    /// Sets the UserPromptSubmit event handler.
    pub fn on_user_prompt_submit(
        mut self,
        handler: impl EventHandle<EventData<UserPromptSubmitPayload>> + 'static,
    ) -> Self {
        self.on_user_prompt_submit = Box::new(handler);
        self
    }

    /// Sets the SessionStart event handler.
    pub fn on_session_start(
        mut self,
        handler: impl EventHandle<EventData<SessionStartPayload>> + 'static,
    ) -> Self {
        self.on_session_start = Box::new(handler);
        self
    }

    /// Sets the SessionEnd event handler.
    pub fn on_session_end(
        mut self,
        handler: impl EventHandle<EventData<SessionEndPayload>> + 'static,
    ) -> Self {
        self.on_session_end = Box::new(handler);
        self
    }

    /// Sets the Stop event handler.
    pub fn on_stop(mut self, handler: impl EventHandle<EventData<StopPayload>> + 'static) -> Self {
        self.on_stop = Box::new(handler);
        self
    }

    /// Sets the StopFailure event handler.
    pub fn on_stop_failure(
        mut self,
        handler: impl EventHandle<EventData<StopFailurePayload>> + 'static,
    ) -> Self {
        self.on_stop_failure = Box::new(handler);
        self
    }

    /// Sets the PreCompact event handler.
    pub fn on_pre_compact(
        mut self,
        handler: impl EventHandle<EventData<PreCompactPayload>> + 'static,
    ) -> Self {
        self.on_pre_compact = Box::new(handler);
        self
    }

    /// Sets the PostCompact event handler.
    pub fn on_post_compact(
        mut self,
        handler: impl EventHandle<EventData<PostCompactPayload>> + 'static,
    ) -> Self {
        self.on_post_compact = Box::new(handler);
        self
    }

    // ---- Notification / Setup / Config builder methods ----

    /// Sets the Notification event handler.
    pub fn on_notification(
        mut self,
        handler: impl EventHandle<EventData<NotificationPayload>> + 'static,
    ) -> Self {
        self.on_notification = Box::new(handler);
        self
    }

    /// Sets the Setup event handler.
    pub fn on_setup(
        mut self,
        handler: impl EventHandle<EventData<SetupPayload>> + 'static,
    ) -> Self {
        self.on_setup = Box::new(handler);
        self
    }

    /// Sets the ConfigChange event handler.
    ///
    /// The hook slot is wired; the `ConfigWatcher` service that emits
    /// `ConfigChangePayload` values is not yet implemented.
    pub fn on_config_change(
        mut self,
        handler: impl EventHandle<EventData<ConfigChangePayload>> + 'static,
    ) -> Self {
        self.on_config_change = Box::new(handler);
        self
    }

    /// Sets the InstructionsLoaded event handler.
    ///
    /// The hook slot is wired; the `CustomInstructionsService` fire
    /// sites that emit `InstructionsLoadedPayload` values are pending.
    pub fn on_instructions_loaded(
        mut self,
        handler: impl EventHandle<EventData<InstructionsLoadedPayload>> + 'static,
    ) -> Self {
        self.on_instructions_loaded = Box::new(handler);
        self
    }

    // ---- Subagent / Permission / File / Worktree builder methods ----

    /// Sets the SubagentStart event handler.
    ///
    /// The hook slot is wired; fire sites in `agent_executor.rs` are
    /// pending until `agent_id` is threaded through the orchestrator.
    pub fn on_subagent_start(
        mut self,
        handler: impl EventHandle<EventData<SubagentStartPayload>> + 'static,
    ) -> Self {
        self.on_subagent_start = Box::new(handler);
        self
    }

    /// Sets the SubagentStop event handler.
    pub fn on_subagent_stop(
        mut self,
        handler: impl EventHandle<EventData<SubagentStopPayload>> + 'static,
    ) -> Self {
        self.on_subagent_stop = Box::new(handler);
        self
    }

    /// Sets the PermissionRequest event handler.
    ///
    /// The hook slot is wired; the fire site in `policy.rs` is pending.
    pub fn on_permission_request(
        mut self,
        handler: impl EventHandle<EventData<PermissionRequestPayload>> + 'static,
    ) -> Self {
        self.on_permission_request = Box::new(handler);
        self
    }

    /// Sets the PermissionDenied event handler.
    pub fn on_permission_denied(
        mut self,
        handler: impl EventHandle<EventData<PermissionDeniedPayload>> + 'static,
    ) -> Self {
        self.on_permission_denied = Box::new(handler);
        self
    }

    /// Sets the CwdChanged event handler.
    ///
    /// The hook slot is wired; the fire site in the Shell tool / cwd
    /// tracker is pending.
    pub fn on_cwd_changed(
        mut self,
        handler: impl EventHandle<EventData<CwdChangedPayload>> + 'static,
    ) -> Self {
        self.on_cwd_changed = Box::new(handler);
        self
    }

    /// Sets the FileChanged event handler.
    ///
    /// The hook slot is wired; the `FileChangedWatcher` service is
    /// pending.
    pub fn on_file_changed(
        mut self,
        handler: impl EventHandle<EventData<FileChangedPayload>> + 'static,
    ) -> Self {
        self.on_file_changed = Box::new(handler);
        self
    }

    /// Sets the WorktreeCreate event handler.
    ///
    /// The hook slot is wired; the worktree tools and sandbox fire
    /// sites are pending.
    pub fn on_worktree_create(
        mut self,
        handler: impl EventHandle<EventData<WorktreeCreatePayload>> + 'static,
    ) -> Self {
        self.on_worktree_create = Box::new(handler);
        self
    }

    /// Sets the WorktreeRemove event handler.
    ///
    /// The hook slot is wired; fire sites are pending.
    pub fn on_worktree_remove(
        mut self,
        handler: impl EventHandle<EventData<WorktreeRemovePayload>> + 'static,
    ) -> Self {
        self.on_worktree_remove = Box::new(handler);
        self
    }

    // ---- MCP elicitation builder methods ----

    /// Sets the Elicitation event handler.
    ///
    /// The hook slot is wired; the MCP client integration that emits
    /// `ElicitationPayload` values is pending.
    pub fn on_elicitation(
        mut self,
        handler: impl EventHandle<EventData<ElicitationPayload>> + 'static,
    ) -> Self {
        self.on_elicitation = Box::new(handler);
        self
    }

    /// Sets the ElicitationResult event handler.
    pub fn on_elicitation_result(
        mut self,
        handler: impl EventHandle<EventData<ElicitationResultPayload>> + 'static,
    ) -> Self {
        self.on_elicitation_result = Box::new(handler);
        self
    }
}

impl Hook {
    /// Combines this hook with another hook, creating a new hook that runs both
    /// handlers in sequence
    ///
    /// When an event is handled, the first hook's handler runs first, then the
    /// second hook's handler runs.
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
            on_pre_tool_use: self.on_pre_tool_use.and(other.on_pre_tool_use),
            on_post_tool_use: self.on_post_tool_use.and(other.on_post_tool_use),
            on_post_tool_use_failure: self
                .on_post_tool_use_failure
                .and(other.on_post_tool_use_failure),
            on_user_prompt_submit: self.on_user_prompt_submit.and(other.on_user_prompt_submit),
            on_session_start: self.on_session_start.and(other.on_session_start),
            on_session_end: self.on_session_end.and(other.on_session_end),
            on_stop: self.on_stop.and(other.on_stop),
            on_stop_failure: self.on_stop_failure.and(other.on_stop_failure),
            on_pre_compact: self.on_pre_compact.and(other.on_pre_compact),
            on_post_compact: self.on_post_compact.and(other.on_post_compact),
            on_notification: self.on_notification.and(other.on_notification),
            on_setup: self.on_setup.and(other.on_setup),
            on_config_change: self.on_config_change.and(other.on_config_change),
            on_instructions_loaded: self
                .on_instructions_loaded
                .and(other.on_instructions_loaded),
            on_subagent_start: self.on_subagent_start.and(other.on_subagent_start),
            on_subagent_stop: self.on_subagent_stop.and(other.on_subagent_stop),
            on_permission_request: self.on_permission_request.and(other.on_permission_request),
            on_permission_denied: self.on_permission_denied.and(other.on_permission_denied),
            on_cwd_changed: self.on_cwd_changed.and(other.on_cwd_changed),
            on_file_changed: self.on_file_changed.and(other.on_file_changed),
            on_worktree_create: self.on_worktree_create.and(other.on_worktree_create),
            on_worktree_remove: self.on_worktree_remove.and(other.on_worktree_remove),
            on_elicitation: self.on_elicitation.and(other.on_elicitation),
            on_elicitation_result: self.on_elicitation_result.and(other.on_elicitation_result),
        }
    }
}

// Implement EventHandle for Hook to allow hooks to handle LifecycleEvent
#[async_trait]
impl EventHandle<LifecycleEvent> for Hook {
    #[allow(deprecated)]
    async fn handle(
        &self,
        event: &LifecycleEvent,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        match &event {
            LifecycleEvent::Start(data) => self.on_start.handle(data, conversation).await,
            LifecycleEvent::End(data) => self.on_end.handle(data, conversation).await,
            LifecycleEvent::Request(data) => self.on_request.handle(data, conversation).await,
            LifecycleEvent::Response(data) => self.on_response.handle(data, conversation).await,
            LifecycleEvent::ToolcallStart(data) => {
                self.on_toolcall_start.handle(data, conversation).await
            }
            LifecycleEvent::ToolcallEnd(data) => {
                self.on_toolcall_end.handle(data, conversation).await
            }
            LifecycleEvent::PreToolUse(data) => {
                self.on_pre_tool_use.handle(data, conversation).await
            }
            LifecycleEvent::PostToolUse(data) => {
                self.on_post_tool_use.handle(data, conversation).await
            }
            LifecycleEvent::PostToolUseFailure(data) => {
                self.on_post_tool_use_failure
                    .handle(data, conversation)
                    .await
            }
            LifecycleEvent::UserPromptSubmit(data) => {
                self.on_user_prompt_submit.handle(data, conversation).await
            }
            LifecycleEvent::SessionStart(data) => {
                self.on_session_start.handle(data, conversation).await
            }
            LifecycleEvent::SessionEnd(data) => {
                self.on_session_end.handle(data, conversation).await
            }
            LifecycleEvent::Stop(data) => self.on_stop.handle(data, conversation).await,
            LifecycleEvent::StopFailure(data) => {
                self.on_stop_failure.handle(data, conversation).await
            }
            LifecycleEvent::PreCompact(data) => {
                self.on_pre_compact.handle(data, conversation).await
            }
            LifecycleEvent::PostCompact(data) => {
                self.on_post_compact.handle(data, conversation).await
            }
            LifecycleEvent::Notification(data) => {
                self.on_notification.handle(data, conversation).await
            }
            LifecycleEvent::Setup(data) => self.on_setup.handle(data, conversation).await,
            LifecycleEvent::ConfigChange(data) => {
                self.on_config_change.handle(data, conversation).await
            }
            LifecycleEvent::InstructionsLoaded(data) => {
                self.on_instructions_loaded.handle(data, conversation).await
            }
            LifecycleEvent::SubagentStart(data) => {
                self.on_subagent_start.handle(data, conversation).await
            }
            LifecycleEvent::SubagentStop(data) => {
                self.on_subagent_stop.handle(data, conversation).await
            }
            LifecycleEvent::PermissionRequest(data) => {
                self.on_permission_request.handle(data, conversation).await
            }
            LifecycleEvent::PermissionDenied(data) => {
                self.on_permission_denied.handle(data, conversation).await
            }
            LifecycleEvent::CwdChanged(data) => {
                self.on_cwd_changed.handle(data, conversation).await
            }
            LifecycleEvent::FileChanged(data) => {
                self.on_file_changed.handle(data, conversation).await
            }
            LifecycleEvent::WorktreeCreate(data) => {
                self.on_worktree_create.handle(data, conversation).await
            }
            LifecycleEvent::WorktreeRemove(data) => {
                self.on_worktree_remove.handle(data, conversation).await
            }
            LifecycleEvent::Elicitation(data) => {
                self.on_elicitation.handle(data, conversation).await
            }
            LifecycleEvent::ElicitationResult(data) => {
                self.on_elicitation_result.handle(data, conversation).await
            }
        }
    }
}

/// A handler that combines two event handlers with sequential execution
///
/// Runs the first handler, then runs the second handler.
///
/// This is used internally by the `Hook::zip` and `EventHandleExt::and`
/// methods.
struct CombinedHandler<T: Send + Sync>(Box<dyn EventHandle<T>>, Box<dyn EventHandle<T>>);

#[async_trait]
impl<T: Send + Sync> EventHandle<T> for CombinedHandler<T> {
    async fn handle(&self, event: &T, conversation: &mut Conversation) -> anyhow::Result<()> {
        // Run the first handler
        self.0.handle(event, conversation).await?;
        // Run the second handler with the cloned event
        self.1.handle(event, conversation).await
    }
}

/// A no-op handler that does nothing
///
/// This is useful as a default handler when you only want to
/// handle specific events.
#[derive(Debug, Default)]
pub struct NoOpHandler;

#[async_trait]
impl<T: Send + Sync> EventHandle<T> for NoOpHandler {
    async fn handle(&self, _: &T, _: &mut Conversation) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait]
impl<T: Send + Sync, F, Fut> EventHandle<T> for F
where
    F: Fn(&T, &mut Conversation) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = anyhow::Result<()>> + Send,
{
    async fn handle(&self, event: &T, conversation: &mut Conversation) -> anyhow::Result<()> {
        (self)(event, conversation).await
    }
}

impl<T: Send + Sync, F, Fut> From<F> for Box<dyn EventHandle<T>>
where
    F: Fn(&T, &mut Conversation) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
{
    fn from(handler: F) -> Self {
        Box::new(handler)
    }
}

#[cfg(test)]
#[allow(deprecated)]
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

    #[tokio::test]
    async fn test_hook_on_start() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let hook = Hook::default().on_start(
            move |event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let events = events_clone.clone();
                let event = event.clone();
                async move {
                    events.lock().unwrap().push(event);
                    Ok(())
                }
            },
        );

        let mut conversation = Conversation::generate();

        hook.handle(
            &LifecycleEvent::Start(EventData::new(test_agent(), test_model_id(), StartPayload)),
            &mut conversation,
        )
        .await
        .unwrap();

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 1);
        assert_eq!(
            handled[0],
            EventData::new(test_agent(), test_model_id(), StartPayload)
        );
    }

    #[tokio::test]
    async fn test_hook_builder() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let hook = Hook::default()
            .on_start({
                let events = events.clone();
                move |event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                    let events = events.clone();
                    let event = LifecycleEvent::Start(event.clone());
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(())
                    }
                }
            })
            .on_end({
                let events = events.clone();
                move |event: &EventData<EndPayload>, _conversation: &mut Conversation| {
                    let events = events.clone();
                    let event = LifecycleEvent::End(event.clone());
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(())
                    }
                }
            })
            .on_request({
                let events = events.clone();
                move |event: &EventData<RequestPayload>, _conversation: &mut Conversation| {
                    let events = events.clone();
                    let event = LifecycleEvent::Request(event.clone());
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(())
                    }
                }
            });

        let mut conversation = Conversation::generate();

        // Test Start event
        hook.handle(
            &LifecycleEvent::Start(EventData::new(test_agent(), test_model_id(), StartPayload)),
            &mut conversation,
        )
        .await
        .unwrap();
        // Test End event
        hook.handle(
            &LifecycleEvent::End(EventData::new(test_agent(), test_model_id(), EndPayload)),
            &mut conversation,
        )
        .await
        .unwrap();
        // Test Request event
        hook.handle(
            &LifecycleEvent::Request(EventData::new(
                test_agent(),
                test_model_id(),
                RequestPayload::new(1),
            )),
            &mut conversation,
        )
        .await
        .unwrap();

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 3);
        assert_eq!(
            handled[0],
            LifecycleEvent::Start(EventData::new(test_agent(), test_model_id(), StartPayload))
        );
        assert_eq!(
            handled[1],
            LifecycleEvent::End(EventData::new(test_agent(), test_model_id(), EndPayload))
        );
        assert_eq!(
            handled[2],
            LifecycleEvent::Request(EventData::new(
                test_agent(),
                test_model_id(),
                RequestPayload::new(1)
            ))
        );
    }

    #[tokio::test]
    async fn test_hook_all_events() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let hook = Hook::new(
            {
                let events = events.clone();
                move |event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                    let events = events.clone();
                    let event = LifecycleEvent::Start(event.clone());
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(())
                    }
                }
            },
            {
                let events = events.clone();
                move |event: &EventData<EndPayload>, _conversation: &mut Conversation| {
                    let events = events.clone();
                    let event = LifecycleEvent::End(event.clone());
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(())
                    }
                }
            },
            {
                let events = events.clone();
                move |event: &EventData<RequestPayload>, _conversation: &mut Conversation| {
                    let events = events.clone();
                    let event = LifecycleEvent::Request(event.clone());
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(())
                    }
                }
            },
            {
                let events = events.clone();
                move |event: &EventData<ResponsePayload>, _conversation: &mut Conversation| {
                    let events = events.clone();
                    let event = LifecycleEvent::Response(event.clone());
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(())
                    }
                }
            },
            {
                let events = events.clone();
                move |event: &EventData<ToolcallStartPayload>, _conversation: &mut Conversation| {
                    let events = events.clone();
                    let event = LifecycleEvent::ToolcallStart(event.clone());
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(())
                    }
                }
            },
            {
                let events = events.clone();
                move |event: &EventData<ToolcallEndPayload>, _conversation: &mut Conversation| {
                    let events = events.clone();
                    let event = LifecycleEvent::ToolcallEnd(event.clone());
                    async move {
                        events.lock().unwrap().push(event);
                        Ok(())
                    }
                }
            },
        );

        let mut conversation = Conversation::generate();

        let all_events = vec![
            LifecycleEvent::Start(EventData::new(test_agent(), test_model_id(), StartPayload)),
            LifecycleEvent::End(EventData::new(test_agent(), test_model_id(), EndPayload)),
            LifecycleEvent::Request(EventData::new(
                test_agent(),
                test_model_id(),
                RequestPayload::new(1),
            )),
            LifecycleEvent::Response(EventData::new(
                test_agent(),
                test_model_id(),
                ResponsePayload::new(ChatCompletionMessageFull {
                    content: "test".to_string(),
                    reasoning: None,
                    tool_calls: vec![],
                    thought_signature: None,
                    reasoning_details: None,
                    usage: crate::Usage::default(),
                    finish_reason: None,
                    phase: None,
                }),
            )),
            LifecycleEvent::ToolcallStart(EventData::new(
                test_agent(),
                test_model_id(),
                ToolcallStartPayload::new(ToolCallFull::new("test_tool")),
            )),
            LifecycleEvent::ToolcallEnd(EventData::new(
                test_agent(),
                test_model_id(),
                ToolcallEndPayload::new(
                    ToolCallFull::new("test_tool"),
                    ToolResult::new("test_tool"),
                ),
            )),
        ];

        for event in all_events {
            hook.handle(&event, &mut conversation).await.unwrap();
        }

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 6);
    }

    #[tokio::test]
    async fn test_step_mutable_conversation() {
        let title = std::sync::Arc::new(std::sync::Mutex::new(None));
        let hook = Hook::default().on_start({
            let title = title.clone();
            move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let title = title.clone();
                async move {
                    *title.lock().unwrap() = Some("Modified title".to_string());
                    Ok(())
                }
            }
        });
        let mut conversation = Conversation::generate();

        assert!(title.lock().unwrap().is_none());

        hook.handle(
            &LifecycleEvent::Start(EventData::new(test_agent(), test_model_id(), StartPayload)),
            &mut conversation,
        )
        .await
        .unwrap();

        assert_eq!(*title.lock().unwrap(), Some("Modified title".to_string()));
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
            move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(())
                }
            }
        });

        let hook2 = Hook::default().on_start({
            let counter = counter2.clone();
            move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(())
                }
            }
        });
        let combined: Hook = hook1.zip(hook2);

        let mut conversation = Conversation::generate();
        combined
            .handle(
                &LifecycleEvent::Start(EventData::new(test_agent(), test_model_id(), StartPayload)),
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
            move |event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let events = events.clone();
                let event = event.clone();
                async move {
                    events.lock().unwrap().push(format!("h1:{:?}", event));
                    Ok(())
                }
            }
        });

        let hook2 = Hook::default().on_start({
            let events = events.clone();
            move |event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let events = events.clone();
                let event = event.clone();
                async move {
                    events.lock().unwrap().push(format!("h2:{:?}", event));
                    Ok(())
                }
            }
        });

        let hook3 = Hook::default().on_start({
            let events = events.clone();
            move |event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let events = events.clone();
                let event = event.clone();
                async move {
                    events.lock().unwrap().push(format!("h3:{:?}", event));
                    Ok(())
                }
            }
        });
        let combined: Hook = hook1.zip(hook2).zip(hook3);

        let mut conversation = Conversation::generate();
        combined
            .handle(
                &LifecycleEvent::Start(EventData::new(test_agent(), test_model_id(), StartPayload)),
                &mut conversation,
            )
            .await
            .unwrap();

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 3);
        assert!(handled[0].starts_with("h1:EventData"));
        assert!(handled[1].starts_with("h2:EventData"));
        assert!(handled[2].starts_with("h3:EventData"));
    }

    #[tokio::test]
    async fn test_hook_zip_different_events() {
        let start_title = std::sync::Arc::new(std::sync::Mutex::new(None));
        let end_title = std::sync::Arc::new(std::sync::Mutex::new(None));

        let hook1 = Hook::default()
            .on_start({
                let start_title = start_title.clone();
                move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                    let start_title = start_title.clone();
                    async move {
                        *start_title.lock().unwrap() = Some("Start".to_string());
                        Ok(())
                    }
                }
            })
            .on_end({
                let end_title = end_title.clone();
                move |_event: &EventData<EndPayload>, _conversation: &mut Conversation| {
                    let end_title = end_title.clone();
                    async move {
                        *end_title.lock().unwrap() = Some("End".to_string());
                        Ok(())
                    }
                }
            });
        let hook2 = Hook::default();

        let combined: Hook = hook1.zip(hook2);

        let mut conversation = Conversation::generate();

        // Test Start event
        combined
            .handle(
                &LifecycleEvent::Start(EventData::new(test_agent(), test_model_id(), StartPayload)),
                &mut conversation,
            )
            .await
            .unwrap();
        assert_eq!(*start_title.lock().unwrap(), Some("Start".to_string()));

        // Test End event
        combined
            .handle(
                &LifecycleEvent::End(EventData::new(test_agent(), test_model_id(), EndPayload)),
                &mut conversation,
            )
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
            move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(())
                }
            }
        };

        let handler2 = {
            let counter = counter2.clone();
            move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(())
                }
            }
        };

        let combined: Box<dyn EventHandle<EventData<StartPayload>>> = handler1.and(handler2);

        let mut conversation = Conversation::generate();
        combined
            .handle(
                &EventData::new(test_agent(), test_model_id(), StartPayload),
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
            move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(())
                }
            }
        };

        let handler2 = {
            let counter = counter2.clone();
            move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(())
                }
            }
        };

        let combined: Box<dyn EventHandle<EventData<StartPayload>>> = handler1.and(handler2);

        let mut conversation = Conversation::generate();
        combined
            .handle(
                &EventData::new(test_agent(), test_model_id(), StartPayload),
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
            move |event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let events = events.clone();
                let event = event.clone();
                async move {
                    events.lock().unwrap().push(format!("h1:{:?}", event));
                    Ok(())
                }
            }
        };

        let handler2 = {
            let events = events.clone();
            move |event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let events = events.clone();
                let event = event.clone();
                async move {
                    events.lock().unwrap().push(format!("h2:{:?}", event));
                    Ok(())
                }
            }
        };

        let handler3 = {
            let events = events.clone();
            move |event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let events = events.clone();
                let event = event.clone();
                async move {
                    events.lock().unwrap().push(format!("h3:{:?}", event));
                    Ok(())
                }
            }
        };

        // Chain handlers using and()
        let combined: Box<dyn EventHandle<EventData<StartPayload>>> =
            handler1.and(handler2).and(handler3);

        let mut conversation = Conversation::generate();
        combined
            .handle(
                &EventData::new(test_agent(), test_model_id(), StartPayload),
                &mut conversation,
            )
            .await
            .unwrap();

        let handled = events.lock().unwrap();
        assert_eq!(handled.len(), 3);
        assert!(handled[0].starts_with("h1:EventData"));
        assert!(handled[1].starts_with("h2:EventData"));
        assert!(handled[2].starts_with("h3:EventData"));
    }

    #[tokio::test]
    async fn test_event_handle_ext_with_hook() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let start_title = std::sync::Arc::new(std::sync::Mutex::new(None));

        let start_handler = {
            let start_title = start_title.clone();
            move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let start_title = start_title.clone();
                async move {
                    *start_title.lock().unwrap() = Some("Started".to_string());
                    Ok(())
                }
            }
        };

        let logging_handler = {
            let events = events.clone();
            move |event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let events = events.clone();
                let event = event.clone();
                async move {
                    events.lock().unwrap().push(format!("Event: {:?}", event));
                    Ok(())
                }
            }
        };

        // Combine handlers using extension trait
        let combined_handler: Box<dyn EventHandle<EventData<StartPayload>>> =
            start_handler.and(logging_handler);

        let hook = Hook::default().on_start(combined_handler);

        let mut conversation = Conversation::generate();
        hook.handle(
            &LifecycleEvent::Start(EventData::new(test_agent(), test_model_id(), StartPayload)),
            &mut conversation,
        )
        .await
        .unwrap();

        assert_eq!(events.lock().unwrap().len(), 1);
        assert!(events.lock().unwrap()[0].starts_with("Event: EventData"));
    }

    #[tokio::test]
    async fn test_hook_as_event_handle() {
        let start_title = std::sync::Arc::new(std::sync::Mutex::new(None));
        let end_title = std::sync::Arc::new(std::sync::Mutex::new(None));

        let hook = Hook::default()
            .on_start({
                let start_title = start_title.clone();
                move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                    let start_title = start_title.clone();
                    async move {
                        *start_title.lock().unwrap() = Some("Started".to_string());
                        Ok(())
                    }
                }
            })
            .on_end({
                let end_title = end_title.clone();
                move |_event: &EventData<EndPayload>, _conversation: &mut Conversation| {
                    let end_title = end_title.clone();
                    async move {
                        *end_title.lock().unwrap() = Some("Ended".to_string());
                        Ok(())
                    }
                }
            });

        // Test using handle() directly (EventHandle trait)
        let mut conversation = Conversation::generate();
        hook.handle(
            &LifecycleEvent::Start(EventData::new(test_agent(), test_model_id(), StartPayload)),
            &mut conversation,
        )
        .await
        .unwrap();
        assert_eq!(*start_title.lock().unwrap(), Some("Started".to_string()));

        hook.handle(
            &LifecycleEvent::End(EventData::new(test_agent(), test_model_id(), EndPayload)),
            &mut conversation,
        )
        .await
        .unwrap();
        assert_eq!(*end_title.lock().unwrap(), Some("Ended".to_string()));
    }

    #[tokio::test]
    async fn test_hook_combination_with_and() {
        let hook1_title = std::sync::Arc::new(std::sync::Mutex::new(None));
        let hook2_title = std::sync::Arc::new(std::sync::Mutex::new(None));

        let handler1 = {
            let hook1_title = hook1_title.clone();
            move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let hook1_title = hook1_title.clone();
                async move {
                    *hook1_title.lock().unwrap() = Some("Started".to_string());
                    Ok(())
                }
            }
        };
        let handler2 = {
            let hook2_title = hook2_title.clone();
            move |_event: &EventData<StartPayload>, _conversation: &mut Conversation| {
                let hook2_title = hook2_title.clone();
                async move {
                    *hook2_title.lock().unwrap() = Some("Ended".to_string());
                    Ok(())
                }
            }
        };

        // Combine handlers using and() extension method
        let combined: Box<dyn EventHandle<EventData<StartPayload>>> = handler1.and(handler2);

        let mut conversation = Conversation::generate();
        combined
            .handle(
                &EventData::new(test_agent(), test_model_id(), StartPayload),
                &mut conversation,
            )
            .await
            .unwrap();

        // Both handlers should have been called
        assert_eq!(*hook1_title.lock().unwrap(), Some("Started".to_string()));
        assert_eq!(*hook2_title.lock().unwrap(), Some("Ended".to_string()));
    }

    // ---- Plugin-hook EventData + Hook tests ----

    #[test]
    fn test_event_data_new_fills_legacy_sentinels() {
        let actual = EventData::new(test_agent(), test_model_id(), StartPayload);

        assert_eq!(actual.session_id, LEGACY_SESSION_ID);
        assert_eq!(
            actual.transcript_path,
            PathBuf::from(LEGACY_TRANSCRIPT_PATH)
        );
        assert_eq!(actual.permission_mode, None);
        // `cwd` is whatever `std::env::current_dir()` returned — don't
        // assert on it beyond being some value.
    }

    #[test]
    fn test_event_data_with_context_sets_explicit_fields() {
        let actual = EventData::with_context(
            test_agent(),
            test_model_id(),
            "sess-xyz",
            PathBuf::from("/tmp/t.jsonl"),
            PathBuf::from("/work"),
            StartPayload,
        );

        assert_eq!(actual.session_id, "sess-xyz");
        assert_eq!(actual.transcript_path, PathBuf::from("/tmp/t.jsonl"));
        assert_eq!(actual.cwd, PathBuf::from("/work"));
        assert_eq!(actual.permission_mode, None);
    }

    #[test]
    fn test_event_data_with_permission_mode_sets_mode() {
        let actual = EventData::with_context(
            test_agent(),
            test_model_id(),
            "s",
            PathBuf::from("/t"),
            PathBuf::from("/c"),
            StartPayload,
        )
        .with_permission_mode("acceptEdits");

        assert_eq!(actual.permission_mode.as_deref(), Some("acceptEdits"));
    }

    #[tokio::test]
    async fn test_hook_on_pre_tool_use_fires_handler() {
        use crate::PreToolUsePayload;

        let fired = std::sync::Arc::new(std::sync::Mutex::new(0u32));
        let hook = Hook::default().on_pre_tool_use({
            let fired = fired.clone();
            move |_event: &EventData<PreToolUsePayload>, _conversation: &mut Conversation| {
                let fired = fired.clone();
                async move {
                    *fired.lock().unwrap() += 1;
                    Ok(())
                }
            }
        });

        let mut conversation = Conversation::generate();
        let event = EventData::new(
            test_agent(),
            test_model_id(),
            PreToolUsePayload {
                tool_name: "Bash".to_string(),
                tool_input: serde_json::json!({"command": "ls"}),
                tool_use_id: "t1".to_string(),
            },
        );
        hook.handle(&LifecycleEvent::PreToolUse(event), &mut conversation)
            .await
            .unwrap();

        assert_eq!(*fired.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_hook_dispatches_new_variants_to_correct_slots() {
        use crate::{
            PostCompactPayload, PostToolUseFailurePayload, PostToolUsePayload, PreCompactPayload,
            PreToolUsePayload, SessionEndPayload, SessionEndReason, SessionStartPayload,
            SessionStartSource, StopFailurePayload, StopPayload, UserPromptSubmitPayload,
        };

        let tag = std::sync::Arc::new(std::sync::Mutex::new(Vec::<&'static str>::new()));

        // Wire every slot to a closure that appends a tag.
        let hook = Hook::default()
            .on_pre_tool_use({
                let tag = tag.clone();
                move |_e: &EventData<PreToolUsePayload>, _c: &mut Conversation| {
                    let tag = tag.clone();
                    async move {
                        tag.lock().unwrap().push("pre_tool_use");
                        Ok(())
                    }
                }
            })
            .on_post_tool_use({
                let tag = tag.clone();
                move |_e: &EventData<PostToolUsePayload>, _c: &mut Conversation| {
                    let tag = tag.clone();
                    async move {
                        tag.lock().unwrap().push("post_tool_use");
                        Ok(())
                    }
                }
            })
            .on_post_tool_use_failure({
                let tag = tag.clone();
                move |_e: &EventData<PostToolUseFailurePayload>, _c: &mut Conversation| {
                    let tag = tag.clone();
                    async move {
                        tag.lock().unwrap().push("post_tool_use_failure");
                        Ok(())
                    }
                }
            })
            .on_user_prompt_submit({
                let tag = tag.clone();
                move |_e: &EventData<UserPromptSubmitPayload>, _c: &mut Conversation| {
                    let tag = tag.clone();
                    async move {
                        tag.lock().unwrap().push("user_prompt_submit");
                        Ok(())
                    }
                }
            })
            .on_session_start({
                let tag = tag.clone();
                move |_e: &EventData<SessionStartPayload>, _c: &mut Conversation| {
                    let tag = tag.clone();
                    async move {
                        tag.lock().unwrap().push("session_start");
                        Ok(())
                    }
                }
            })
            .on_session_end({
                let tag = tag.clone();
                move |_e: &EventData<SessionEndPayload>, _c: &mut Conversation| {
                    let tag = tag.clone();
                    async move {
                        tag.lock().unwrap().push("session_end");
                        Ok(())
                    }
                }
            })
            .on_stop({
                let tag = tag.clone();
                move |_e: &EventData<StopPayload>, _c: &mut Conversation| {
                    let tag = tag.clone();
                    async move {
                        tag.lock().unwrap().push("stop");
                        Ok(())
                    }
                }
            })
            .on_stop_failure({
                let tag = tag.clone();
                move |_e: &EventData<StopFailurePayload>, _c: &mut Conversation| {
                    let tag = tag.clone();
                    async move {
                        tag.lock().unwrap().push("stop_failure");
                        Ok(())
                    }
                }
            })
            .on_pre_compact({
                let tag = tag.clone();
                move |_e: &EventData<PreCompactPayload>, _c: &mut Conversation| {
                    let tag = tag.clone();
                    async move {
                        tag.lock().unwrap().push("pre_compact");
                        Ok(())
                    }
                }
            })
            .on_post_compact({
                let tag = tag.clone();
                move |_e: &EventData<PostCompactPayload>, _c: &mut Conversation| {
                    let tag = tag.clone();
                    async move {
                        tag.lock().unwrap().push("post_compact");
                        Ok(())
                    }
                }
            });

        let mut conversation = Conversation::generate();
        let agent = test_agent();
        let mid = test_model_id();

        // Fire one of each.
        let events = vec![
            LifecycleEvent::PreToolUse(EventData::new(
                agent.clone(),
                mid.clone(),
                PreToolUsePayload {
                    tool_name: "Bash".to_string(),
                    tool_input: serde_json::json!({}),
                    tool_use_id: "t1".to_string(),
                },
            )),
            LifecycleEvent::PostToolUse(EventData::new(
                agent.clone(),
                mid.clone(),
                PostToolUsePayload {
                    tool_name: "Bash".to_string(),
                    tool_input: serde_json::json!({}),
                    tool_response: serde_json::json!({}),
                    tool_use_id: "t1".to_string(),
                },
            )),
            LifecycleEvent::PostToolUseFailure(EventData::new(
                agent.clone(),
                mid.clone(),
                PostToolUseFailurePayload {
                    tool_name: "Bash".to_string(),
                    tool_input: serde_json::json!({}),
                    tool_use_id: "t1".to_string(),
                    error: "boom".to_string(),
                    is_interrupt: None,
                },
            )),
            LifecycleEvent::UserPromptSubmit(EventData::new(
                agent.clone(),
                mid.clone(),
                UserPromptSubmitPayload { prompt: "hi".to_string() },
            )),
            LifecycleEvent::SessionStart(EventData::new(
                agent.clone(),
                mid.clone(),
                SessionStartPayload { source: SessionStartSource::Startup, model: None },
            )),
            LifecycleEvent::SessionEnd(EventData::new(
                agent.clone(),
                mid.clone(),
                SessionEndPayload { reason: SessionEndReason::Clear },
            )),
            LifecycleEvent::Stop(EventData::new(
                agent.clone(),
                mid.clone(),
                StopPayload { stop_hook_active: false, last_assistant_message: None },
            )),
            LifecycleEvent::StopFailure(EventData::new(
                agent.clone(),
                mid.clone(),
                StopFailurePayload {
                    error: "x".to_string(),
                    error_details: None,
                    last_assistant_message: None,
                },
            )),
            LifecycleEvent::PreCompact(EventData::new(
                agent.clone(),
                mid.clone(),
                PreCompactPayload {
                    trigger: crate::CompactTrigger::Manual,
                    custom_instructions: None,
                },
            )),
            LifecycleEvent::PostCompact(EventData::new(
                agent.clone(),
                mid.clone(),
                PostCompactPayload {
                    trigger: crate::CompactTrigger::Auto,
                    compact_summary: "ok".to_string(),
                },
            )),
        ];

        for event in events {
            hook.handle(&event, &mut conversation).await.unwrap();
        }

        let handled = tag.lock().unwrap();
        assert_eq!(
            handled.clone(),
            vec![
                "pre_tool_use",
                "post_tool_use",
                "post_tool_use_failure",
                "user_prompt_submit",
                "session_start",
                "session_end",
                "stop",
                "stop_failure",
                "pre_compact",
                "post_compact",
            ]
        );
    }
}
