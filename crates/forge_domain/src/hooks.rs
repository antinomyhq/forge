//! Orchestrator lifecycle hooks.

use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

use derive_setters::Setters;

use crate::{Agent, ChatCompletionMessageFull, Context, ToolCallFull, ToolResult};

/// Result type for hook operations.
#[derive(Debug, Clone)]
#[must_use = "HookResult should be handled"]
pub enum HookResult<T> {
    /// Continue with the (possibly modified) value.
    Continue(T),
    /// Abort the orchestrator run with an error.
    Abort(String),
}

impl<T> HookResult<T> {
    /// Returns true if this is a `Continue` variant.
    pub fn is_continue(&self) -> bool {
        matches!(self, HookResult::Continue(_))
    }

    /// Returns true if this is an `Abort` variant.
    pub fn is_abort(&self) -> bool {
        matches!(self, HookResult::Abort(_))
    }

    /// Converts to `Result<T, String>` for use with `?` operator.
    pub fn into_result(self) -> Result<T, String> {
        match self {
            HookResult::Continue(v) => Ok(v),
            HookResult::Abort(e) => Err(e),
        }
    }

    /// Converts to `anyhow::Result<T>` with a hook name for context.
    pub fn into_anyhow(self, hook_name: &str) -> anyhow::Result<T> {
        self.into_result()
            .map_err(|e| anyhow::anyhow!("{}: {}", hook_name, e))
    }
}

/// Result from `pre_chat` hook with the context and whether to stop the loop.
#[derive(Debug, Clone)]
pub struct ChatAction {
    /// The (possibly modified) context to use for the chat.
    pub context: Context,
    /// If true, stop the loop after this iteration.
    pub stop: bool,
}

impl ChatAction {
    /// Continue looping with the given context.
    pub fn cont(context: Context) -> Self {
        Self { context, stop: false }
    }

    /// Process this context, then stop the loop.
    pub fn stop(context: Context) -> Self {
        Self { context, stop: true }
    }
}

/// Type alias for boxed async futures used in hooks.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Context provided to the `init` hook.
#[derive(Debug, Clone, Setters)]
#[setters(into, strip_option)]
pub struct InitContext {
    /// The agent being initialized.
    pub agent: Agent,
    /// The initial context for the conversation.
    pub context: Context,
}

/// Context provided to the `pre_chat` hook.
#[derive(Debug, Clone, Setters)]
#[setters(into, strip_option)]
pub struct PreChatContext {
    /// The agent making the chat request.
    pub agent: Agent,
    /// The context being sent to the provider.
    pub context: Context,
    /// The current iteration count in the orchestrator loop.
    pub iteration: usize,
}

/// Context provided to the `post_chat` hook.
#[derive(Debug, Clone, Setters)]
#[setters(into, strip_option)]
pub struct PostChatContext {
    /// The agent that made the chat request.
    pub agent: Agent,
    /// The context that was sent to the provider.
    pub context: Context,
    /// The response message from the provider.
    pub message: ChatCompletionMessageFull,
    /// The current iteration count in the orchestrator loop.
    pub iteration: usize,
}

/// Context provided to the `pre_tool_call` hook.
#[derive(Debug, Clone, Setters)]
#[setters(into, strip_option)]
pub struct PreToolCallContext {
    /// The agent executing the tool.
    pub agent: Agent,
    /// The tool call to be executed.
    pub tool_call: ToolCallFull,
}

/// Context provided to the `post_tool_call` hook.
#[derive(Debug, Clone, Setters)]
#[setters(into, strip_option)]
pub struct PostToolCallContext {
    /// The agent that executed the tool.
    pub agent: Agent,
    /// The tool call that was executed.
    pub tool_call: ToolCallFull,
    /// The result of the tool execution.
    pub result: ToolResult,
}

/// Context provided to the `complete` hook.
#[derive(Debug, Clone, Setters)]
#[setters(into, strip_option)]
pub struct CompleteContext {
    /// The agent that completed the task.
    pub agent: Agent,
    /// The final context after all iterations.
    pub context: Context,
    /// Whether the task completed successfully (vs interrupted).
    pub is_complete: bool,
    /// Total number of iterations performed.
    pub total_iterations: usize,
}

/// Trait for implementing orchestrator lifecycle hooks.
pub trait OrchHook: Send + Sync + Debug {
    /// Called once when the agent is initialized, before the main loop starts.
    fn init<'a>(&'a self, ctx: InitContext) -> BoxFuture<'a, HookResult<Context>> {
        Box::pin(async move { HookResult::Continue(ctx.context) })
    }

    /// Called before making a call to the provider.
    fn pre_chat<'a>(&'a self, ctx: PreChatContext) -> BoxFuture<'a, HookResult<ChatAction>> {
        Box::pin(async move { HookResult::Continue(ChatAction::cont(ctx.context)) })
    }

    /// Called after the call to the provider has succeeded.
    fn post_chat<'a>(
        &'a self,
        ctx: PostChatContext,
    ) -> BoxFuture<'a, HookResult<ChatCompletionMessageFull>> {
        Box::pin(async move { HookResult::Continue(ctx.message) })
    }

    /// Called before each tool is executed.
    fn pre_tool_call<'a>(
        &'a self,
        ctx: PreToolCallContext,
    ) -> BoxFuture<'a, HookResult<ToolCallFull>> {
        Box::pin(async move { HookResult::Continue(ctx.tool_call) })
    }

    /// Called after each tool execution completes.
    fn post_tool_call<'a>(
        &'a self,
        ctx: PostToolCallContext,
    ) -> BoxFuture<'a, HookResult<ToolResult>> {
        Box::pin(async move { HookResult::Continue(ctx.result) })
    }

    /// Called after the loop is completed and during final cleanup.
    fn complete<'a>(&'a self, ctx: CompleteContext) -> BoxFuture<'a, ()> {
        let _ = ctx;
        Box::pin(async {})
    }
}

/// A no-op hook implementation that passes through all values unchanged.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpHook;

impl OrchHook for NoOpHook {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentId, ModelId, ProviderId};

    fn test_agent() -> Agent {
        Agent::new(
            AgentId::new("test"),
            ProviderId::OPENAI,
            ModelId::new("test"),
        )
    }

    #[tokio::test]
    async fn test_no_op_hook_passes_through() {
        let hook = NoOpHook;
        let ctx = InitContext { agent: test_agent(), context: Context::default() };
        let result = hook.init(ctx).await;
        assert!(result.is_continue());
    }

    #[tokio::test]
    async fn test_hook_result_abort() {
        let result: HookResult<i32> = HookResult::Abort("error".into());
        assert!(result.is_abort());
        assert_eq!(result.into_result().unwrap_err(), "error");
    }

    #[tokio::test]
    async fn test_hook_result_continue() {
        let result: HookResult<i32> = HookResult::Continue(42);
        assert!(result.is_continue());
        assert_eq!(result.into_result().unwrap(), 42);
    }

    #[tokio::test]
    async fn test_pre_chat_continues_by_default() {
        let hook = NoOpHook;
        let ctx = PreChatContext {
            agent: test_agent(),
            context: Context::default(),
            iteration: 0,
        };
        let result = hook.pre_chat(ctx).await.into_result().unwrap();
        assert!(!result.stop);
    }

    #[derive(Debug)]
    struct StopOnIterationHook {
        stop_at: usize,
    }

    impl OrchHook for StopOnIterationHook {
        fn pre_chat<'a>(&'a self, ctx: PreChatContext) -> BoxFuture<'a, HookResult<ChatAction>> {
            let stop_at = self.stop_at;
            Box::pin(async move {
                if ctx.iteration >= stop_at {
                    HookResult::Continue(ChatAction::stop(ctx.context))
                } else {
                    HookResult::Continue(ChatAction::cont(ctx.context))
                }
            })
        }
    }

    #[tokio::test]
    async fn test_pre_chat_can_stop_loop() {
        let hook = StopOnIterationHook { stop_at: 5 };
        let ctx = PreChatContext {
            agent: test_agent(),
            context: Context::default(),
            iteration: 5,
        };
        let result = hook.pre_chat(ctx).await.into_result().unwrap();
        assert!(result.stop);
    }
}
