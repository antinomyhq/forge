//! Shared logic for LLM-based hook executors (prompt and agent hooks).
//!
//! Both prompt hooks and agent hooks use a single LLM call with a
//! configurable system prompt and timeout. This module provides the
//! common execution logic, response parsing, and `$ARGUMENTS`
//! substitution.

use forge_app::HookExecutorInfra;
use forge_domain::{
    Context, ContextMessage, HookDecision, HookExecResult, HookInput, HookOutput, ModelId,
    ResponseFormat, SyncHookOutput,
};

use crate::hook_runtime::HookOutcome;

/// JSON schema for the hook response: `{ "ok": bool, "reason"?: string }`.
pub(crate) fn hook_response_schema() -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "properties": {
            "ok": { "type": "boolean" },
            "reason": { "type": "string" }
        },
        "required": ["ok"],
        "additionalProperties": false
    })
}

/// Replace `$ARGUMENTS` in the prompt text with the JSON-serialized
/// hook input. Matches Claude Code's `addArgumentsToPrompt()` from
/// `claude-code/src/utils/hooks/hookHelpers.ts:6-30`.
pub(crate) fn substitute_arguments(prompt: &str, input: &HookInput) -> String {
    if !prompt.contains("$ARGUMENTS") {
        return prompt.to_string();
    }
    // Serialize the full input as JSON for substitution.
    let json_input = serde_json::to_string(input).unwrap_or_default();
    prompt.replace("$ARGUMENTS", &json_input)
}

/// Parsed model response.
#[derive(serde::Deserialize)]
struct HookResponse {
    ok: bool,
    reason: Option<String>,
}

/// Configuration for a single LLM hook execution.
pub(crate) struct LlmHookConfig<'a> {
    /// The prompt text (may contain `$ARGUMENTS`).
    pub prompt: &'a str,
    /// Optional model override.
    pub model: Option<&'a str>,
    /// Optional timeout override in seconds.
    pub timeout: Option<u64>,
    /// The system prompt to use.
    pub system_prompt: &'a str,
    /// Default model ID when not overridden.
    pub default_model: &'a str,
    /// Default timeout in seconds when not overridden.
    pub default_timeout_secs: u64,
    /// Label for log messages (e.g. "Prompt hook", "Agent hook").
    pub hook_label: &'a str,
}

/// Execute a single-shot LLM hook call with the given configuration.
///
/// Shared implementation for both prompt hooks and agent hooks.
pub(crate) async fn execute_llm_hook(
    config: LlmHookConfig<'_>,
    input: &HookInput,
    executor: &dyn HookExecutorInfra,
) -> anyhow::Result<HookExecResult> {
    // 1. Substitute $ARGUMENTS in the prompt text.
    let processed_prompt = substitute_arguments(config.prompt, input);

    // 2. Determine the model to use.
    let model_id = ModelId::new(config.model.unwrap_or(config.default_model));

    // 3. Build the LLM context.
    let context = Context::default()
        .add_message(ContextMessage::system(config.system_prompt.to_string()))
        .add_message(ContextMessage::user(
            processed_prompt.clone(),
            Some(model_id.clone()),
        ))
        .response_format(ResponseFormat::JsonSchema(Box::new(hook_response_schema())));

    // 4. Apply timeout.
    let timeout_secs = config.timeout.unwrap_or(config.default_timeout_secs);
    let timeout_duration = std::time::Duration::from_secs(timeout_secs);

    // 5. Make the LLM call with timeout.
    let llm_result = tokio::time::timeout(
        timeout_duration,
        executor.query_model_for_hook(&model_id, context),
    )
    .await;

    match llm_result {
        // Timeout — cancelled outcome.
        Err(_elapsed) => {
            tracing::warn!(
                prompt = %config.prompt,
                timeout_secs,
                "{} timed out", config.hook_label
            );
            Ok(HookExecResult {
                outcome: HookOutcome::Cancelled,
                output: None,
                raw_stdout: String::new(),
                raw_stderr: format!("{} timed out after {}s", config.hook_label, timeout_secs),
                exit_code: None,
            })
        }
        // LLM call error — non-blocking error.
        Ok(Err(err)) => {
            let err_msg = format!(
                "Error executing {}: {err}",
                config.hook_label.to_lowercase()
            );
            tracing::warn!(
                prompt = %config.prompt,
                error = %err,
                "{} LLM call failed", config.hook_label
            );
            Ok(HookExecResult {
                outcome: HookOutcome::NonBlockingError,
                output: None,
                raw_stdout: String::new(),
                raw_stderr: err_msg,
                exit_code: Some(1),
            })
        }
        // LLM call succeeded — parse the response.
        Ok(Ok(response_text)) => {
            let trimmed = response_text.trim();
            tracing::debug!(
                prompt = %config.prompt,
                response = %trimmed,
                "{} model response", config.hook_label
            );

            // Try to parse the JSON response.
            let parsed: Result<HookResponse, _> = serde_json::from_str(trimmed);
            match parsed {
                Err(parse_err) => {
                    tracing::warn!(
                        response = %trimmed,
                        error = %parse_err,
                        "{} response is not valid JSON", config.hook_label
                    );
                    Ok(HookExecResult {
                        outcome: HookOutcome::NonBlockingError,
                        output: None,
                        raw_stdout: trimmed.to_string(),
                        raw_stderr: format!("JSON validation failed: {parse_err}"),
                        exit_code: Some(1),
                    })
                }
                Ok(hook_resp) if hook_resp.ok => {
                    // Condition was met — success.
                    tracing::debug!(
                        prompt = %config.prompt,
                        "{} condition was met", config.hook_label
                    );
                    Ok(HookExecResult {
                        outcome: HookOutcome::Success,
                        output: Some(HookOutput::Sync(SyncHookOutput {
                            should_continue: Some(true),
                            ..Default::default()
                        })),
                        raw_stdout: trimmed.to_string(),
                        raw_stderr: String::new(),
                        exit_code: Some(0),
                    })
                }
                Ok(hook_resp) => {
                    // Condition was not met — blocking.
                    let reason = hook_resp.reason.unwrap_or_default();
                    tracing::info!(
                        prompt = %config.prompt,
                        reason = %reason,
                        "{} condition was not met", config.hook_label
                    );
                    let output = HookOutput::Sync(SyncHookOutput {
                        should_continue: Some(false),
                        decision: Some(HookDecision::Block),
                        reason: Some(format!(
                            "{} condition was not met: {reason}",
                            config.hook_label
                        )),
                        ..Default::default()
                    });
                    Ok(HookExecResult {
                        outcome: HookOutcome::Blocking,
                        output: Some(output),
                        raw_stdout: trimmed.to_string(),
                        raw_stderr: String::new(),
                        exit_code: Some(1),
                    })
                }
            }
        }
    }
}
