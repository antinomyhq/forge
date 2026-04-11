//! Prompt hook executor — single LLM call evaluation.
//!
//! A prompt hook sends one LLM request with a hardcoded system prompt,
//! the hook's prompt text (with `$ARGUMENTS` substituted), and parses
//! the model's `{ "ok": true }` / `{ "ok": false, "reason": "..." }`
//! response to decide whether to allow or block the action.
//!
//! Reference: `claude-code/src/utils/hooks/execPromptHook.ts`

use forge_app::HookExecutorInfra;
use forge_domain::{HookExecResult, HookInput, PromptHookCommand};

use crate::hook_runtime::llm_common::{self, LlmHookConfig};

/// Default model for prompt hooks when the config doesn't specify one.
/// Matches Claude Code's `getSmallFastModel()`.
const DEFAULT_PROMPT_HOOK_MODEL: &str = "claude-3-5-haiku-20241022";

/// Default timeout for prompt hooks in seconds.
const DEFAULT_PROMPT_HOOK_TIMEOUT_SECS: u64 = 30;

/// System prompt for evaluating hook conditions via LLM.
/// Exact match of Claude Code's `execPromptHook.ts:65-69`.
const HOOK_EVALUATION_SYSTEM_PROMPT: &str = r#"You are evaluating a hook in Claude Code.

Your response must be a JSON object matching one of the following schemas:
1. If the condition is met, return: {"ok": true}
2. If the condition is not met, return: {"ok": false, "reason": "Reason for why it is not met"}"#;

/// Executor for prompt hooks.
///
/// Uses a single LLM call to evaluate whether a hook condition is met.
/// The model receives the hook prompt (with `$ARGUMENTS` substituted)
/// and must respond with `{"ok": true}` or `{"ok": false, "reason": "..."}`.
#[derive(Debug, Clone, Default)]
pub struct ForgePromptHookExecutor;

impl ForgePromptHookExecutor {
    /// Execute a prompt hook by making a single LLM call.
    ///
    /// # Arguments
    /// - `config` — The prompt hook configuration (prompt text, model override,
    ///   timeout).
    /// - `input` — The hook input payload (tool name, args, etc.).
    /// - `executor` — The executor infra providing `query_model_for_hook`.
    pub async fn execute(
        &self,
        config: &PromptHookCommand,
        input: &HookInput,
        executor: &dyn HookExecutorInfra,
    ) -> anyhow::Result<HookExecResult> {
        llm_common::execute_llm_hook(
            LlmHookConfig {
                prompt: &config.prompt,
                model: config.model.as_deref(),
                timeout: config.timeout,
                system_prompt: HOOK_EVALUATION_SYSTEM_PROMPT,
                default_model: DEFAULT_PROMPT_HOOK_MODEL,
                default_timeout_secs: DEFAULT_PROMPT_HOOK_TIMEOUT_SECS,
                hook_label: "Prompt hook",
            },
            input,
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_domain::{HookInputBase, HookInputPayload, HookOutput};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::hook_runtime::HookOutcome;
    use crate::hook_runtime::llm_common::substitute_arguments;
    use crate::hook_runtime::test_mocks::mocks::{
        ErrorLlmExecutor, HangingLlmExecutor, MockLlmExecutor,
    };

    fn sample_input() -> forge_domain::HookInput {
        forge_domain::HookInput {
            base: HookInputBase {
                session_id: "sess-prompt".to_string(),
                transcript_path: PathBuf::from("/tmp/transcript.json"),
                cwd: PathBuf::from("/tmp"),
                permission_mode: None,
                agent_id: None,
                agent_type: None,
                hook_event_name: "UserPromptSubmit".to_string(),
            },
            payload: HookInputPayload::UserPromptSubmit { prompt: "hello".to_string() },
        }
    }

    fn prompt_hook() -> PromptHookCommand {
        PromptHookCommand {
            prompt: "Summarize: $ARGUMENTS".to_string(),
            condition: None,
            timeout: None,
            model: None,
            status_message: None,
            once: false,
        }
    }

    #[test]
    fn test_substitute_arguments_replaces_placeholder() {
        let input = sample_input();
        let result = substitute_arguments("Check: $ARGUMENTS", &input);
        assert!(result.contains("UserPromptSubmit"));
        assert!(result.contains("hello"));
        assert!(!result.contains("$ARGUMENTS"));
    }

    #[test]
    fn test_substitute_arguments_no_placeholder() {
        let input = sample_input();
        let result = substitute_arguments("Just a plain prompt", &input);
        assert_eq!(result, "Just a plain prompt");
    }

    #[tokio::test]
    async fn test_prompt_hook_ok_true() {
        let executor = MockLlmExecutor::with_response(r#"{"ok": true}"#);
        let prompt_executor = ForgePromptHookExecutor;
        let hook = prompt_hook();

        let result = prompt_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);
        assert!(result.output.is_some());
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_prompt_hook_ok_false_with_reason() {
        let executor =
            MockLlmExecutor::with_response(r#"{"ok": false, "reason": "Tests are failing"}"#);
        let prompt_executor = ForgePromptHookExecutor;
        let hook = prompt_hook();

        let result = prompt_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Blocking);
        assert_eq!(result.exit_code, Some(1));
        if let Some(HookOutput::Sync(sync)) = &result.output {
            assert_eq!(sync.should_continue, Some(false));
            assert!(sync.reason.as_ref().unwrap().contains("Tests are failing"));
        } else {
            panic!("Expected Sync output");
        }
    }

    #[tokio::test]
    async fn test_prompt_hook_invalid_json_response() {
        let executor = MockLlmExecutor::with_response("not valid json at all");
        let prompt_executor = ForgePromptHookExecutor;
        let hook = prompt_hook();

        let result = prompt_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::NonBlockingError);
        assert!(result.raw_stderr.contains("JSON validation failed"));
        assert_eq!(result.exit_code, Some(1));
    }

    #[tokio::test]
    async fn test_prompt_hook_llm_error() {
        let executor = ErrorLlmExecutor;
        let prompt_executor = ForgePromptHookExecutor;
        let hook = prompt_hook();

        let result = prompt_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::NonBlockingError);
        assert!(result.raw_stderr.contains("Error executing prompt hook"));
        assert!(result.raw_stderr.contains("connection refused"));
        assert_eq!(result.exit_code, Some(1));
    }

    #[tokio::test]
    async fn test_prompt_hook_timeout() {
        let executor = HangingLlmExecutor;
        let prompt_executor = ForgePromptHookExecutor;
        let mut hook = prompt_hook();
        hook.timeout = Some(1); // 1 second timeout

        let result = prompt_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Cancelled);
        assert!(result.raw_stderr.contains("timed out"));
    }

    #[tokio::test]
    async fn test_prompt_hook_custom_model() {
        let executor = Arc::new(MockLlmExecutor::with_response(r#"{"ok": true}"#));
        let prompt_executor = ForgePromptHookExecutor;
        let mut hook = prompt_hook();
        hook.model = Some("claude-3-opus-20240229".to_string());

        prompt_executor
            .execute(&hook, &sample_input(), executor.as_ref())
            .await
            .unwrap();

        assert_eq!(
            *executor.captured_model.lock().unwrap(),
            Some("claude-3-opus-20240229".to_string())
        );
    }

    #[tokio::test]
    async fn test_prompt_hook_default_model() {
        let executor = Arc::new(MockLlmExecutor::with_response(r#"{"ok": true}"#));
        let prompt_executor = ForgePromptHookExecutor;
        let hook = prompt_hook();

        prompt_executor
            .execute(&hook, &sample_input(), executor.as_ref())
            .await
            .unwrap();

        assert_eq!(
            *executor.captured_model.lock().unwrap(),
            Some(DEFAULT_PROMPT_HOOK_MODEL.to_string())
        );
    }

    #[tokio::test]
    async fn test_prompt_hook_ok_false_without_reason() {
        let executor = MockLlmExecutor::with_response(r#"{"ok": false}"#);
        let prompt_executor = ForgePromptHookExecutor;
        let hook = prompt_hook();

        let result = prompt_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Blocking);
        if let Some(HookOutput::Sync(sync)) = &result.output {
            assert!(sync.reason.as_ref().unwrap().contains("not met"));
        }
    }

    #[test]
    fn test_hook_response_schema_is_valid() {
        // Ensure the schema is valid JSON Schema.
        let schema = crate::hook_runtime::llm_common::hook_response_schema();
        let json = serde_json::to_value(schema).unwrap();
        assert_eq!(json["type"], "object");
        assert!(json["properties"]["ok"]["type"] == "boolean");
    }
}
