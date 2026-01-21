//! Orchestrator lifecycle hooks.

use async_trait::async_trait;
use std::fmt::Debug;

use derive_setters::Setters;

use crate::{Agent, ChatCompletionMessageFull, Context, ToolCallFull, ToolResult};

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
#[async_trait]
pub trait OrchHook: Send + Sync + Debug {
    /// Called once when the agent is initialized, before the main loop starts.
    async fn init(&self, ctx: InitContext) -> Result<Context, String> {
        Ok(ctx.context)
    }

    /// Called before making a call to the provider.
    async fn pre_chat(&self, ctx: PreChatContext) -> Result<ChatAction, String> {
        Ok(ChatAction::cont(ctx.context))
    }

    /// Called after the call to the provider has succeeded.
    async fn post_chat(
        &self,
        ctx: PostChatContext,
    ) -> Result<ChatCompletionMessageFull, String> {
        Ok(ctx.message)
    }

    /// Called before each tool is executed.
    async fn pre_tool_call(
        &self,
        ctx: PreToolCallContext,
    ) -> Result<ToolCallFull, String> {
        Ok(ctx.tool_call)
    }

    /// Called after each tool execution completes.
    async fn post_tool_call(
        &self,
        ctx: PostToolCallContext,
    ) -> Result<ToolResult, String> {
        Ok(ctx.result)
    }

    /// Called after the loop is completed and during final cleanup.
    async fn complete(&self, ctx: CompleteContext) {
        let _ = ctx;
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
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_hook_result_abort() {
        let result: Result<i32, String> = Err("error".into());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "error");
    }

    #[tokio::test]
    async fn test_hook_result_continue() {
        let result: Result<i32, String> = Ok(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_pre_chat_continues_by_default() {
        let hook = NoOpHook;
        let ctx = PreChatContext {
            agent: test_agent(),
            context: Context::default(),
            iteration: 0,
        };
        let result = hook.pre_chat(ctx).await.unwrap();
        assert!(!result.stop);
    }

    #[derive(Debug)]
    struct StopOnIterationHook {
        stop_at: usize,
    }

    #[async_trait]
    impl OrchHook for StopOnIterationHook {
        async fn pre_chat(&self, ctx: PreChatContext) -> Result<ChatAction, String> {
            let stop_at = self.stop_at;
            if ctx.iteration >= stop_at {
                Ok(ChatAction::stop(ctx.context))
            } else {
                Ok(ChatAction::cont(ctx.context))
            }
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
        let result = hook.pre_chat(ctx).await.unwrap();
        assert!(result.stop);
    }
}
