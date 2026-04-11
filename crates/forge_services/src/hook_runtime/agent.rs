//! Agent hook executor — multi-turn LLM verification.
//!
//! An agent hook uses a multi-turn LLM loop to verify stop conditions
//! (e.g. "verify that the tests pass"). The model receives the hook's
//! prompt text (with `$ARGUMENTS` substituted) and a verification-focused
//! system prompt that includes the transcript path. The model has
//! multiple turns to produce `{"ok": true}` or `{"ok": false,
//! "reason": "..."}`, with automatic retry on malformed responses.
//!
//! Unlike prompt hooks (single LLM call), agent hooks support up to
//! `MAX_AGENT_TURNS` (50) rounds. A future enhancement will add tool
//! access (Read, Shell, etc.) so the sub-agent can inspect the
//! codebase directly.
//!
//! Reference: `claude-code/src/utils/hooks/execAgentHook.ts`

use forge_app::HookExecutorInfra;
use forge_domain::{
    AgentHookCommand, Context, ContextMessage, HookDecision, HookExecResult, HookInput, HookOutput,
    ModelId, ResponseFormat, SyncHookOutput,
};

use crate::hook_runtime::HookOutcome;
use crate::hook_runtime::llm_common::substitute_arguments;

/// Default model for agent hooks when the config doesn't specify one.
/// Matches Claude Code's `getSmallFastModel()`.
const DEFAULT_AGENT_HOOK_MODEL: &str = "claude-3-5-haiku-20241022";

/// Default timeout for agent hooks in seconds.
/// Agent hooks get a longer timeout than prompt hooks (60 s vs 30 s)
/// because they are intended for richer verification scenarios.
const DEFAULT_AGENT_HOOK_TIMEOUT_SECS: u64 = 60;

/// Maximum number of LLM turns for an agent hook before giving up.
const MAX_AGENT_TURNS: usize = 50;

/// System prompt for agent hook condition verification.
/// Based on Claude Code's `execAgentHook.ts:107-115`.
const AGENT_HOOK_SYSTEM_PROMPT: &str = r#"You are verifying a stop condition in Claude Code. Your task is to verify that the agent completed the given plan.

Use as few steps as possible - be efficient and direct.

Your response must be a JSON object matching one of the following schemas:
1. If the condition is met, return: {"ok": true}
2. If the condition is not met, return: {"ok": false, "reason": "Reason for why it is not met"}"#;

/// Executor for agent hooks.
///
/// Uses a multi-turn LLM loop to verify whether a stop condition is
/// met. The model receives the hook prompt (with `$ARGUMENTS`
/// substituted) and a condition-verification system prompt, then has
/// up to [`MAX_AGENT_TURNS`] attempts to produce `{"ok": true}` or
/// `{"ok": false, "reason": "..."}`. Malformed responses trigger an
/// automatic retry with a corrective user message.
#[derive(Debug, Clone, Default)]
pub struct ForgeAgentHookExecutor;

impl ForgeAgentHookExecutor {
    /// Execute an agent hook using a multi-turn LLM loop.
    ///
    /// # Arguments
    /// - `config` — The agent hook configuration (prompt text, model override,
    ///   timeout).
    /// - `input` — The hook input payload (tool name, args, etc.).
    /// - `executor` — The executor infra providing `execute_agent_loop`.
    pub async fn execute(
        &self,
        config: &AgentHookCommand,
        input: &HookInput,
        executor: &dyn HookExecutorInfra,
    ) -> anyhow::Result<HookExecResult> {
        let processed_prompt = substitute_arguments(&config.prompt, input);
        let model_id = ModelId::new(config.model.as_deref().unwrap_or(DEFAULT_AGENT_HOOK_MODEL));

        // Build system prompt with transcript path for context.
        let system_prompt = format!(
            "{base}\n\nThe conversation transcript is available at: {path}",
            base = AGENT_HOOK_SYSTEM_PROMPT,
            path = input.base.transcript_path.display(),
        );

        let context = Context::default()
            .add_message(ContextMessage::system(system_prompt))
            .add_message(ContextMessage::user(
                processed_prompt.clone(),
                Some(model_id.clone()),
            ))
            .response_format(ResponseFormat::JsonSchema(Box::new(
                crate::hook_runtime::llm_common::hook_response_schema(),
            )));

        let timeout_secs = config.timeout.unwrap_or(DEFAULT_AGENT_HOOK_TIMEOUT_SECS);
        let timeout_duration = std::time::Duration::from_secs(timeout_secs);

        let llm_result = tokio::time::timeout(
            timeout_duration,
            executor.execute_agent_loop(&model_id, context, MAX_AGENT_TURNS, timeout_secs),
        )
        .await;

        match llm_result {
            Err(_elapsed) => {
                // Timeout
                Ok(HookExecResult {
                    outcome: HookOutcome::Cancelled,
                    output: None,
                    raw_stdout: String::new(),
                    raw_stderr: format!("Agent hook timed out after {}s", timeout_secs),
                    exit_code: None,
                })
            }
            Ok(Err(err)) => {
                // LLM error
                Ok(HookExecResult {
                    outcome: HookOutcome::NonBlockingError,
                    output: None,
                    raw_stdout: String::new(),
                    raw_stderr: format!("Error executing agent hook: {err}"),
                    exit_code: Some(1),
                })
            }
            Ok(Ok(None)) => {
                // Max turns without structured output
                Ok(HookExecResult {
                    outcome: HookOutcome::Cancelled,
                    output: None,
                    raw_stdout: String::new(),
                    raw_stderr: "Agent hook exhausted max turns without providing a result"
                        .to_string(),
                    exit_code: None,
                })
            }
            Ok(Ok(Some((true, _reason)))) => {
                // Condition met
                Ok(HookExecResult {
                    outcome: HookOutcome::Success,
                    output: Some(HookOutput::Sync(SyncHookOutput {
                        should_continue: Some(true),
                        ..Default::default()
                    })),
                    raw_stdout: String::new(),
                    raw_stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            Ok(Ok(Some((false, reason)))) => {
                // Condition not met
                let reason_str = reason.unwrap_or_default();
                let output = HookOutput::Sync(SyncHookOutput {
                    should_continue: Some(false),
                    decision: Some(HookDecision::Block),
                    reason: Some(format!("Agent hook condition was not met: {reason_str}")),
                    ..Default::default()
                });
                Ok(HookExecResult {
                    outcome: HookOutcome::Blocking,
                    output: Some(output),
                    raw_stdout: String::new(),
                    raw_stderr: String::new(),
                    exit_code: Some(1),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_domain::{HookInputBase, HookInputPayload, HookOutput};
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::hook_runtime::HookOutcome;
    use crate::hook_runtime::llm_common::substitute_arguments;
    use crate::hook_runtime::test_mocks::mocks::{
        ErrorLlmExecutor, HangingLlmExecutor, MockLlmExecutor,
    };

    fn sample_input() -> forge_domain::HookInput {
        forge_domain::HookInput {
            base: HookInputBase {
                session_id: "sess-agent".to_string(),
                transcript_path: PathBuf::from("/tmp/transcript.json"),
                cwd: PathBuf::from("/tmp"),
                permission_mode: None,
                agent_id: None,
                agent_type: None,
                hook_event_name: "PreToolUse".to_string(),
            },
            payload: HookInputPayload::PreToolUse {
                tool_name: "Bash".to_string(),
                tool_input: json!({"command": "cargo test"}),
                tool_use_id: "toolu_agent".to_string(),
            },
        }
    }

    fn agent_hook() -> AgentHookCommand {
        AgentHookCommand {
            prompt: "Verify tests pass".to_string(),
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
        assert!(result.contains("PreToolUse"));
        assert!(result.contains("cargo test"));
        assert!(!result.contains("$ARGUMENTS"));
    }

    #[test]
    fn test_substitute_arguments_no_placeholder() {
        let input = sample_input();
        let result = substitute_arguments("Just a plain prompt", &input);
        assert_eq!(result, "Just a plain prompt");
    }

    #[tokio::test]
    async fn test_agent_hook_ok_true() {
        let executor = MockLlmExecutor::with_response(r#"{"ok": true}"#);
        let agent_executor = ForgeAgentHookExecutor;
        let hook = agent_hook();

        let result = agent_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);
        assert!(result.output.is_some());
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_agent_hook_ok_false_with_reason() {
        let executor =
            MockLlmExecutor::with_response(r#"{"ok": false, "reason": "Tests are failing"}"#);
        let agent_executor = ForgeAgentHookExecutor;
        let hook = agent_hook();

        let result = agent_executor
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
    async fn test_agent_hook_ok_false_without_reason() {
        let executor = MockLlmExecutor::with_response(r#"{"ok": false}"#);
        let agent_executor = ForgeAgentHookExecutor;
        let hook = agent_hook();

        let result = agent_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Blocking);
        if let Some(HookOutput::Sync(sync)) = &result.output {
            assert!(sync.reason.as_ref().unwrap().contains("not met"));
        }
    }

    #[tokio::test]
    async fn test_agent_hook_invalid_json_exhausts_turns() {
        let executor = MockLlmExecutor::with_response("not valid json at all");
        let agent_executor = ForgeAgentHookExecutor;
        let hook = agent_hook();

        let result = agent_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        // With multi-turn, invalid JSON means the agent loop returned None
        // (max turns exhausted without valid response).
        assert_eq!(result.outcome, HookOutcome::Cancelled);
        assert!(result.raw_stderr.contains("exhausted max turns"));
    }

    #[tokio::test]
    async fn test_agent_hook_llm_error() {
        let executor = ErrorLlmExecutor;
        let agent_executor = ForgeAgentHookExecutor;
        let hook = agent_hook();

        let result = agent_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::NonBlockingError);
        assert!(result.raw_stderr.contains("Error executing agent hook"));
        assert!(result.raw_stderr.contains("connection refused"));
        assert_eq!(result.exit_code, Some(1));
    }

    #[tokio::test]
    async fn test_agent_hook_timeout() {
        let executor = HangingLlmExecutor;
        let agent_executor = ForgeAgentHookExecutor;
        let mut hook = agent_hook();
        hook.timeout = Some(1); // 1 second timeout

        let result = agent_executor
            .execute(&hook, &sample_input(), &executor)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Cancelled);
        assert!(result.raw_stderr.contains("timed out"));
    }

    #[tokio::test]
    async fn test_agent_hook_custom_model() {
        let executor = Arc::new(MockLlmExecutor::with_response(r#"{"ok": true}"#));
        let agent_executor = ForgeAgentHookExecutor;
        let mut hook = agent_hook();
        hook.model = Some("claude-3-opus-20240229".to_string());

        agent_executor
            .execute(&hook, &sample_input(), executor.as_ref())
            .await
            .unwrap();

        assert_eq!(
            *executor.captured_model.lock().unwrap(),
            Some("claude-3-opus-20240229".to_string())
        );
    }

    #[tokio::test]
    async fn test_agent_hook_default_model() {
        let executor = Arc::new(MockLlmExecutor::with_response(r#"{"ok": true}"#));
        let agent_executor = ForgeAgentHookExecutor;
        let hook = agent_hook();

        agent_executor
            .execute(&hook, &sample_input(), executor.as_ref())
            .await
            .unwrap();

        assert_eq!(
            *executor.captured_model.lock().unwrap(),
            Some(DEFAULT_AGENT_HOOK_MODEL.to_string())
        );
    }

    #[test]
    fn test_hook_response_schema_is_valid() {
        let schema = crate::hook_runtime::llm_common::hook_response_schema();
        let json = serde_json::to_value(schema).unwrap();
        assert_eq!(json["type"], "object");
        assert!(json["properties"]["ok"]["type"] == "boolean");
    }
}
