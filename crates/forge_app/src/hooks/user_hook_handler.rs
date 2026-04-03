use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use forge_domain::{
    ContextMessage, Conversation, EndPayload, EventData, EventHandle, HookEventInput,
    HookExecutionResult, HookInput, HookOutput, RequestPayload, ResponsePayload, Role,
    StartPayload, ToolCallArguments, ToolcallEndPayload, ToolcallStartPayload, UserHookConfig,
    UserHookEntry, UserHookEventName, UserHookMatcherGroup,
};
use regex::Regex;
use serde_json::Value;
use tracing::{debug, warn};

use super::user_hook_executor::UserHookExecutor;
use crate::services::HookCommandService;

/// Default timeout for hook commands (10 minutes).
const DEFAULT_HOOK_TIMEOUT: Duration = Duration::from_secs(600);

/// EventHandle implementation that bridges user-configured hooks with the
/// existing lifecycle event system.
///
/// This handler is constructed from a `UserHookConfig` and executes matching
/// hook commands at each lifecycle event point. It wires into the existing
/// `Hook` system via `Hook::zip()`.
#[derive(Clone)]
pub struct UserHookHandler<I> {
    executor: UserHookExecutor<I>,
    config: UserHookConfig,
    cwd: PathBuf,
    env_vars: HashMap<String, String>,
    /// Tracks whether a Stop hook has already fired to prevent infinite loops.
    stop_hook_active: std::sync::Arc<AtomicBool>,
}

impl<I> UserHookHandler<I> {
    /// Creates a new user hook handler from configuration.
    ///
    /// # Arguments
    /// * `service` - The hook command service used to execute hook commands.
    /// * `config` - The merged user hook configuration.
    /// * `cwd` - Current working directory for command execution.
    /// * `project_dir` - Project root directory for `FORGE_PROJECT_DIR` env
    ///   var.
    /// * `session_id` - Current session/conversation ID.
    /// * `default_hook_timeout` - Default timeout in milliseconds for hook
    ///   commands.
    pub fn new(
        service: I,
        mut env_vars: BTreeMap<String, String>,
        config: UserHookConfig,
        cwd: PathBuf,
        session_id: String,
    ) -> Self {
        env_vars.insert(
            "FORGE_PROJECT_DIR".to_string(),
            cwd.to_string_lossy().to_string(),
        );
        env_vars.insert("FORGE_SESSION_ID".to_string(), session_id);
        env_vars.insert("FORGE_CWD".to_string(), cwd.to_string_lossy().to_string());

        Self {
            executor: UserHookExecutor::new(service),
            config,
            cwd,
            env_vars: env_vars.into_iter().collect(),
            stop_hook_active: std::sync::Arc::new(AtomicBool::new(false)),
        }
    }

    /// Checks if the config has any hooks for the given event.
    fn has_hooks(&self, event: &UserHookEventName) -> bool {
        !self.config.get_groups(event).is_empty()
    }

    /// Finds matching hook entries for an event, filtered by the optional
    /// matcher regex against the given subject string.
    fn find_matching_hooks<'a>(
        groups: &'a [UserHookMatcherGroup],
        subject: Option<&str>,
    ) -> Vec<&'a UserHookEntry> {
        let mut matching = Vec::new();

        for group in groups {
            let matches = match (&group.matcher, subject) {
                (Some(pattern), Some(subj)) => match Regex::new(pattern) {
                    Ok(re) => re.is_match(subj),
                    Err(e) => {
                        warn!(
                            pattern = pattern,
                            error = %e,
                            "Invalid regex in hook matcher, skipping"
                        );
                        false
                    }
                },
                (Some(_), None) => {
                    // Matcher specified but no subject to match against; skip
                    false
                }
                (None, _) => {
                    // No matcher means unconditional match
                    true
                }
            };

            if matches {
                matching.extend(group.hooks.iter());
            }
        }

        matching
    }

    /// Executes a list of hook entries and returns their results.
    async fn execute_hooks(
        &self,
        hooks: &[&UserHookEntry],
        input: &HookInput,
    ) -> Vec<HookExecutionResult>
    where
        I: HookCommandService,
    {
        let input_json = match serde_json::to_string(input) {
            Ok(json) => json,
            Err(e) => {
                warn!(error = %e, "Failed to serialize hook input");
                return Vec::new();
            }
        };

        let mut results = Vec::new();
        for hook in hooks {
            if let Some(command) = &hook.command {
                match self
                    .executor
                    .execute(
                        command,
                        &input_json,
                        hook.timeout
                            .map(Duration::from_millis)
                            .unwrap_or(DEFAULT_HOOK_TIMEOUT),
                        &self.cwd,
                        &self.env_vars,
                    )
                    .await
                {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        warn!(
                            command = command,
                            error = %e,
                            "Hook command failed to execute"
                        );
                    }
                }
            }
        }

        results
    }

    /// Processes hook results, returning a blocking reason if any hook blocked.
    fn process_results(results: &[HookExecutionResult]) -> Option<String> {
        for result in results {
            // Exit code 2 = blocking error
            if result.is_blocking_exit() {
                let message = result
                    .blocking_message()
                    .unwrap_or("Hook blocked execution")
                    .to_string();
                return Some(message);
            }

            // Exit code 0 = check stdout for JSON decisions
            if let Some(output) = result.parse_output()
                && output.is_blocking()
            {
                let reason = output
                    .reason
                    .unwrap_or_else(|| "Hook blocked execution".to_string());
                return Some(reason);
            }

            // Non-blocking errors (exit code 1, etc.) are logged but don't block
            if result.is_non_blocking_error() {
                warn!(
                    exit_code = ?result.exit_code,
                    stderr = result.stderr.as_str(),
                    "Hook command returned non-blocking error"
                );
            }
        }

        None
    }

    /// Processes PreToolUse results, extracting updated input if present.
    fn process_pre_tool_use_output(results: &[HookExecutionResult]) -> PreToolUseDecision {
        for result in results {
            // Exit code 2 = blocking error
            if result.is_blocking_exit() {
                let message = result
                    .blocking_message()
                    .unwrap_or("Hook blocked tool execution")
                    .to_string();
                return PreToolUseDecision::Block(message);
            }

            // Exit code 0 = check stdout for JSON decisions
            if let Some(output) = result.parse_output() {
                // Check permission decision
                if output.permission_decision.as_deref() == Some("deny") {
                    let reason = output
                        .reason
                        .unwrap_or_else(|| "Tool execution denied by hook".to_string());
                    return PreToolUseDecision::Block(reason);
                }

                // Check generic block decision
                if output.is_blocking() {
                    let reason = output
                        .reason
                        .unwrap_or_else(|| "Hook blocked tool execution".to_string());
                    return PreToolUseDecision::Block(reason);
                }

                // Check for updated input
                if output.updated_input.is_some() {
                    return PreToolUseDecision::AllowWithUpdate(output);
                }
            }

            // Non-blocking errors are logged but don't block
            if result.is_non_blocking_error() {
                warn!(
                    exit_code = ?result.exit_code,
                    stderr = result.stderr.as_str(),
                    "PreToolUse hook command returned non-blocking error"
                );
            }
        }

        PreToolUseDecision::Allow
    }
}

/// Decision result from PreToolUse hook processing.
enum PreToolUseDecision {
    /// Allow the tool call to proceed.
    Allow,
    /// Allow but with updated input from the hook output.
    AllowWithUpdate(HookOutput),
    /// Block the tool call with the given reason.
    Block(String),
}

// --- EventHandle implementations ---

#[async_trait]
impl<I: HookCommandService> EventHandle<EventData<StartPayload>> for UserHookHandler<I> {
    async fn handle(
        &self,
        _event: &mut EventData<StartPayload>,
        _conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        if !self.has_hooks(&UserHookEventName::SessionStart) {
            return Ok(());
        }

        let groups = self.config.get_groups(&UserHookEventName::SessionStart);
        let hooks = Self::find_matching_hooks(groups, Some("startup"));

        if hooks.is_empty() {
            return Ok(());
        }

        let input = HookInput {
            hook_event_name: UserHookEventName::SessionStart.to_string(),
            cwd: self.cwd.to_string_lossy().to_string(),
            session_id: self.env_vars.get("FORGE_SESSION_ID").cloned(),
            event_data: HookEventInput::SessionStart { source: "startup".to_string() },
        };

        let results = self.execute_hooks(&hooks, &input).await;

        // FIXME: SessionStart hooks can provide additional context but not block;
        // additional_context is detected here but never injected into the conversation.
        for result in &results {
            if let Some(output) = result.parse_output()
                && let Some(context) = &output.additional_context
            {
                debug!(
                    context_len = context.len(),
                    "SessionStart hook provided additional context"
                );
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<I: HookCommandService> EventHandle<EventData<RequestPayload>> for UserHookHandler<I> {
    async fn handle(
        &self,
        event: &mut EventData<RequestPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        // Only fire on the first request of a turn (user-submitted prompt).
        // Subsequent iterations are internal LLM retry/tool-call loops and
        // should not re-trigger UserPromptSubmit.
        if event.payload.request_count != 0 {
            return Ok(());
        }

        if !self.has_hooks(&UserHookEventName::UserPromptSubmit) {
            return Ok(());
        }

        let groups = self.config.get_groups(&UserHookEventName::UserPromptSubmit);
        let hooks = Self::find_matching_hooks(groups, None);

        if hooks.is_empty() {
            return Ok(());
        }

        // Extract the last user message text as the prompt sent to the hook.
        let prompt = conversation
            .context
            .as_ref()
            .and_then(|ctx| {
                ctx.messages
                    .iter()
                    .rev()
                    .find(|m| m.has_role(Role::User))
                    .and_then(|m| m.content())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();

        let input = HookInput {
            hook_event_name: "UserPromptSubmit".to_string(),
            cwd: self.cwd.to_string_lossy().to_string(),
            session_id: self.env_vars.get("FORGE_SESSION_ID").cloned(),
            event_data: HookEventInput::UserPromptSubmit { prompt },
        };

        let results = self.execute_hooks(&hooks, &input).await;

        if let Some(reason) = Self::process_results(&results) {
            debug!(
                reason = reason.as_str(),
                "UserPromptSubmit hook blocked with feedback"
            );
            // Inject feedback so the model sees why the prompt was flagged.
            if let Some(context) = conversation.context.as_mut() {
                let feedback_msg = format!(
                    "<hook_feedback>\n<event>UserPromptSubmit</event>\n<status>blocked</status>\n<reason>{reason}</reason>\n</hook_feedback>"
                );
                context
                    .messages
                    .push(ContextMessage::user(feedback_msg, None).into());
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<I: HookCommandService> EventHandle<EventData<ResponsePayload>> for UserHookHandler<I> {
    async fn handle(
        &self,
        _event: &mut EventData<ResponsePayload>,
        _conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        // FIXME: No user hook events map to Response currently
        Ok(())
    }
}

#[async_trait]
impl<I: HookCommandService> EventHandle<EventData<ToolcallStartPayload>> for UserHookHandler<I> {
    async fn handle(
        &self,
        event: &mut EventData<ToolcallStartPayload>,
        _conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        if !self.has_hooks(&UserHookEventName::PreToolUse) {
            return Ok(());
        }

        // Use owned String to avoid borrow conflicts when mutating event later.
        let tool_name = event.payload.tool_call.name.as_str().to_string();
        // FIXME: Add a tool name transformer to map tool names to Forge
        // equivalents (e.g. "Bash" → "shell") so that hook configs written
        let groups = self.config.get_groups(&UserHookEventName::PreToolUse);
        let hooks = Self::find_matching_hooks(groups, Some(tool_name.as_str()));

        if hooks.is_empty() {
            return Ok(());
        }

        let tool_input =
            serde_json::to_value(&event.payload.tool_call.arguments).unwrap_or_default();

        let input = HookInput {
            hook_event_name: "PreToolUse".to_string(),
            cwd: self.cwd.to_string_lossy().to_string(),
            session_id: self.env_vars.get("FORGE_SESSION_ID").cloned(),
            event_data: HookEventInput::PreToolUse {
                tool_name: tool_name.clone(),
                tool_input,
            },
        };

        let results = self.execute_hooks(&hooks, &input).await;
        let decision = Self::process_pre_tool_use_output(&results);

        match decision {
            PreToolUseDecision::Allow => Ok(()),
            PreToolUseDecision::AllowWithUpdate(output) => {
                if let Some(updated_input) = output.updated_input {
                    event.payload.tool_call.arguments =
                        ToolCallArguments::Parsed(Value::Object(updated_input));
                    debug!(
                        tool_name = tool_name.as_str(),
                        "PreToolUse hook updated tool input"
                    );
                }
                Ok(())
            }
            PreToolUseDecision::Block(reason) => {
                debug!(
                    tool_name = tool_name.as_str(),
                    reason = reason.as_str(),
                    "PreToolUse hook blocked tool call"
                );
                // Return an error to signal the orchestrator to skip this tool call.
                // The orchestrator converts this into an error ToolResult visible to
                // the model.
                Err(anyhow::anyhow!(
                    "Tool call '{}' blocked by PreToolUse hook: {}",
                    tool_name,
                    reason
                ))
            }
        }
    }
}

#[async_trait]
impl<I: HookCommandService> EventHandle<EventData<ToolcallEndPayload>> for UserHookHandler<I> {
    async fn handle(
        &self,
        event: &mut EventData<ToolcallEndPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let is_error = event.payload.result.is_error();
        let event_name = if is_error {
            UserHookEventName::PostToolUseFailure
        } else {
            UserHookEventName::PostToolUse
        };

        if !self.has_hooks(&event_name) {
            return Ok(());
        }

        let tool_name = event.payload.tool_call.name.as_str();
        let groups = self.config.get_groups(&event_name);
        let hooks = Self::find_matching_hooks(groups, Some(tool_name));

        if hooks.is_empty() {
            return Ok(());
        }

        let tool_input =
            serde_json::to_value(&event.payload.tool_call.arguments).unwrap_or_default();
        let tool_response = serde_json::to_value(&event.payload.result.output).unwrap_or_default();

        let input = HookInput {
            hook_event_name: event_name.to_string(),
            cwd: self.cwd.to_string_lossy().to_string(),
            session_id: self.env_vars.get("FORGE_SESSION_ID").cloned(),
            event_data: HookEventInput::PostToolUse {
                tool_name: tool_name.to_string(),
                tool_input,
                tool_response,
            },
        };

        let results = self.execute_hooks(&hooks, &input).await;

        // PostToolUse can provide feedback via blocking
        if let Some(reason) = Self::process_results(&results) {
            debug!(
                tool_name = tool_name,
                event = %event_name,
                reason = reason.as_str(),
                "PostToolUse hook blocked with feedback"
            );
            // Inject feedback as a user message
            if let Some(context) = conversation.context.as_mut() {
                let feedback_msg = format!(
                    "<hook_feedback>\n<event>{}</event>\n<tool>{}</tool>\n<status>blocked</status>\n<reason>{}</reason>\n</hook_feedback>",
                    event_name, tool_name, reason
                );
                context
                    .messages
                    .push(forge_domain::ContextMessage::user(feedback_msg, None).into());
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<I: HookCommandService> EventHandle<EventData<EndPayload>> for UserHookHandler<I> {
    async fn handle(
        &self,
        _event: &mut EventData<EndPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        // Fire SessionEnd hooks
        if self.has_hooks(&UserHookEventName::SessionEnd) {
            let groups = self.config.get_groups(&UserHookEventName::SessionEnd);
            let hooks = Self::find_matching_hooks(groups, None);

            if !hooks.is_empty() {
                let input = HookInput {
                    hook_event_name: "SessionEnd".to_string(),
                    cwd: self.cwd.to_string_lossy().to_string(),
                    session_id: self.env_vars.get("FORGE_SESSION_ID").cloned(),
                    event_data: HookEventInput::Empty {},
                };
                self.execute_hooks(&hooks, &input).await;
            }
        }

        // Fire Stop hooks
        if !self.has_hooks(&UserHookEventName::Stop) {
            return Ok(());
        }

        // Prevent infinite loops
        let was_active = self.stop_hook_active.swap(true, Ordering::SeqCst);
        if was_active {
            debug!("Stop hook already active, skipping to prevent infinite loop");
            return Ok(());
        }

        let groups = self.config.get_groups(&UserHookEventName::Stop);
        let hooks = Self::find_matching_hooks(groups, None);

        if hooks.is_empty() {
            self.stop_hook_active.store(false, Ordering::SeqCst);
            return Ok(());
        }

        let input = HookInput {
            hook_event_name: "Stop".to_string(),
            cwd: self.cwd.to_string_lossy().to_string(),
            session_id: self.env_vars.get("FORGE_SESSION_ID").cloned(),
            event_data: HookEventInput::Stop { stop_hook_active: was_active },
        };

        let results = self.execute_hooks(&hooks, &input).await;

        if let Some(reason) = Self::process_results(&results) {
            debug!(
                reason = reason.as_str(),
                "Stop hook wants to continue conversation"
            );
            // Inject a message to continue the conversation
            if let Some(context) = conversation.context.as_mut() {
                let continue_msg = format!(
                    "<hook_feedback>\n<event>Stop</event>\n<status>continue</status>\n<reason>{}</reason>\n</hook_feedback>",
                    reason
                );
                context
                    .messages
                    .push(forge_domain::ContextMessage::user(continue_msg, None).into());
            }
        }

        // Reset the stop hook active flag
        self.stop_hook_active.store(false, Ordering::SeqCst);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use forge_domain::{
        CommandOutput, HookExecutionResult, UserHookEntry, UserHookEventName, UserHookMatcherGroup,
        UserHookType,
    };
    use pretty_assertions::assert_eq;

    use super::*;

    /// A no-op service stub for tests that only exercise config/matching logic.
    #[derive(Clone)]
    struct NullInfra;

    #[async_trait::async_trait]
    impl HookCommandService for NullInfra {
        async fn execute_command_with_input(
            &self,
            command: String,
            _working_dir: PathBuf,
            _stdin_input: String,
            _env_vars: HashMap<String, String>,
        ) -> anyhow::Result<CommandOutput> {
            Ok(CommandOutput {
                command,
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn null_handler(config: UserHookConfig) -> UserHookHandler<NullInfra> {
        UserHookHandler::new(
            NullInfra,
            BTreeMap::new(),
            config,
            PathBuf::from("/tmp"),
            "sess-1".to_string(),
        )
    }

    fn make_entry(command: &str) -> UserHookEntry {
        UserHookEntry {
            hook_type: UserHookType::Command,
            command: Some(command.to_string()),
            timeout: None,
        }
    }

    fn make_group(matcher: Option<&str>, commands: &[&str]) -> UserHookMatcherGroup {
        UserHookMatcherGroup {
            matcher: matcher.map(|s| s.to_string()),
            hooks: commands.iter().map(|c| make_entry(c)).collect(),
        }
    }

    #[test]
    fn test_find_matching_hooks_no_matcher_fires_unconditionally() {
        let groups = vec![make_group(None, &["echo hi"])];
        let actual = UserHookHandler::<NullInfra>::find_matching_hooks(&groups, Some("Bash"));
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].command, Some("echo hi".to_string()));
    }

    #[test]
    fn test_find_matching_hooks_no_matcher_fires_without_subject() {
        let groups = vec![make_group(None, &["echo hi"])];
        let actual = UserHookHandler::<NullInfra>::find_matching_hooks(&groups, None);
        assert_eq!(actual.len(), 1);
    }

    #[test]
    fn test_find_matching_hooks_regex_match() {
        let groups = vec![make_group(Some("Bash"), &["block.sh"])];
        let actual = UserHookHandler::<NullInfra>::find_matching_hooks(&groups, Some("Bash"));
        assert_eq!(actual.len(), 1);
    }

    #[test]
    fn test_find_matching_hooks_regex_no_match() {
        let groups = vec![make_group(Some("Bash"), &["block.sh"])];
        let actual = UserHookHandler::<NullInfra>::find_matching_hooks(&groups, Some("Write"));
        assert!(actual.is_empty());
    }

    #[test]
    fn test_find_matching_hooks_regex_partial_match() {
        let groups = vec![make_group(Some("Bash|Write"), &["check.sh"])];
        let actual = UserHookHandler::<NullInfra>::find_matching_hooks(&groups, Some("Bash"));
        assert_eq!(actual.len(), 1);
    }

    #[test]
    fn test_find_matching_hooks_matcher_but_no_subject() {
        let groups = vec![make_group(Some("Bash"), &["block.sh"])];
        let actual = UserHookHandler::<NullInfra>::find_matching_hooks(&groups, None);
        assert!(actual.is_empty());
    }

    #[test]
    fn test_find_matching_hooks_invalid_regex_skipped() {
        let groups = vec![make_group(Some("[invalid"), &["block.sh"])];
        let actual = UserHookHandler::<NullInfra>::find_matching_hooks(&groups, Some("anything"));
        assert!(actual.is_empty());
    }

    #[test]
    fn test_find_matching_hooks_multiple_groups() {
        let groups = vec![
            make_group(Some("Bash"), &["bash-hook.sh"]),
            make_group(Some("Write"), &["write-hook.sh"]),
            make_group(None, &["always.sh"]),
        ];
        let actual = UserHookHandler::<NullInfra>::find_matching_hooks(&groups, Some("Bash"));
        assert_eq!(actual.len(), 2); // Bash match + unconditional
    }

    #[test]
    fn test_process_pre_tool_use_output_allow_on_success() {
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        assert!(matches!(actual, PreToolUseDecision::Allow));
    }

    #[test]
    fn test_process_pre_tool_use_output_block_on_exit_2() {
        let results = vec![HookExecutionResult {
            exit_code: Some(2),
            stdout: String::new(),
            stderr: "Blocked: dangerous command".to_string(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        assert!(
            matches!(actual, PreToolUseDecision::Block(msg) if msg.contains("dangerous command"))
        );
    }

    #[test]
    fn test_process_pre_tool_use_output_block_on_deny() {
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: r#"{"permissionDecision": "deny", "reason": "Not allowed"}"#.to_string(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        assert!(matches!(actual, PreToolUseDecision::Block(msg) if msg == "Not allowed"));
    }

    #[test]
    fn test_process_pre_tool_use_output_block_on_decision() {
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: r#"{"decision": "block", "reason": "Blocked by policy"}"#.to_string(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        assert!(matches!(actual, PreToolUseDecision::Block(msg) if msg == "Blocked by policy"));
    }

    #[test]
    fn test_process_pre_tool_use_output_non_blocking_error_allows() {
        let results = vec![HookExecutionResult {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "some error".to_string(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        assert!(matches!(actual, PreToolUseDecision::Allow));
    }

    #[test]
    fn test_process_results_no_blocking() {
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_results(&results);
        assert!(actual.is_none());
    }

    #[test]
    fn test_process_results_blocking_exit_code() {
        let results = vec![HookExecutionResult {
            exit_code: Some(2),
            stdout: String::new(),
            stderr: "stop reason".to_string(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_results(&results);
        assert_eq!(actual, Some("stop reason".to_string()));
    }

    #[test]
    fn test_process_results_blocking_json_decision() {
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: r#"{"decision": "block", "reason": "keep going"}"#.to_string(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_results(&results);
        assert_eq!(actual, Some("keep going".to_string()));
    }

    #[test]
    fn test_has_hooks_returns_false_for_empty_config() {
        let config = UserHookConfig::new();
        let handler = null_handler(config);
        assert!(!handler.has_hooks(&UserHookEventName::PreToolUse));
    }

    #[test]
    fn test_has_hooks_returns_true_when_configured() {
        let json = r#"{"PreToolUse": [{"hooks": [{"type": "command", "command": "echo hi"}]}]}"#;
        let config: UserHookConfig = serde_json::from_str(json).unwrap();
        let handler = null_handler(config);
        assert!(handler.has_hooks(&UserHookEventName::PreToolUse));
        assert!(!handler.has_hooks(&UserHookEventName::Stop));
    }

    #[test]
    fn test_process_pre_tool_use_output_allow_with_update_detected() {
        // A hook that returns updatedInput should produce AllowWithUpdate with the
        // correct updated_input value.
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: r#"{"updatedInput": {"command": "echo safe"}}"#.to_string(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        let expected_map = serde_json::Map::from_iter([("command".to_string(), serde_json::json!("echo safe"))]);
        assert!(
            matches!(&actual, PreToolUseDecision::AllowWithUpdate(output) if output.updated_input == Some(expected_map))
        );
    }

    #[tokio::test]
    async fn test_allow_with_update_modifies_tool_call_arguments() {
        // When a PreToolUse hook returns updatedInput, the handler must
        // overwrite event.payload.tool_call.arguments with the new value.
        use forge_domain::{
            Agent, EventData, ModelId, ProviderId, ToolCallArguments, ToolCallFull,
            ToolcallStartPayload,
        };

        let json = r#"{"PreToolUse": [{"hooks": [{"type": "command", "command": "echo hi"}]}]}"#;
        let config: UserHookConfig = serde_json::from_str(json).unwrap();

        // NullInfra returns exit_code=0 with empty stdout (Allow), so we need a custom
        // infra that returns updatedInput JSON.
        #[derive(Clone)]
        struct UpdateInfra;

        #[async_trait::async_trait]
        impl HookCommandService for UpdateInfra {
            async fn execute_command_with_input(
                &self,
                command: String,
                _working_dir: PathBuf,
                _stdin_input: String,
                _env_vars: HashMap<String, String>,
            ) -> anyhow::Result<forge_domain::CommandOutput> {
                Ok(forge_domain::CommandOutput {
                    command,
                    exit_code: Some(0),
                    stdout: r#"{"updatedInput": {"command": "echo safe"}}"#.to_string(),
                    stderr: String::new(),
                })
            }
        }

        let handler = UserHookHandler::new(
            UpdateInfra,
            BTreeMap::new(),
            config,
            PathBuf::from("/tmp"),
            "sess-test".to_string(),
        );

        let agent = Agent::new(
            "test-agent",
            ProviderId::from("test-provider".to_string()),
            ModelId::new("test-model"),
        );
        let original_args = ToolCallArguments::from_json(r#"{"command": "rm -rf /"}"#);
        let tool_call = ToolCallFull::new("shell").arguments(original_args);
        let mut event = EventData::new(
            agent,
            ModelId::new("test-model"),
            ToolcallStartPayload::new(tool_call),
        );
        let mut conversation = forge_domain::Conversation::generate();

        handler.handle(&mut event, &mut conversation).await.unwrap();

        let actual_args = event.payload.tool_call.arguments.parse().unwrap();
        let expected_args = serde_json::json!({"command": "echo safe"});
        assert_eq!(actual_args, expected_args);
    }

    #[test]
    fn test_allow_with_update_none_updated_input_leaves_args_unchanged() {
        // When HookOutput has updated_input = None (e.g. only
        // `{"permissionDecision": "allow"}`), AllowWithUpdate should not
        // overwrite the original arguments.
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: r#"{"permissionDecision": "allow"}"#.to_string(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        // permissionDecision "allow" with no updatedInput => plain Allow
        assert!(matches!(actual, PreToolUseDecision::Allow));
    }

    #[test]
    fn test_allow_with_update_empty_object() {
        // updatedInput is an empty object — still a valid update.
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: r#"{"updatedInput": {}}"#.to_string(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        let expected_map = serde_json::Map::new();
        assert!(
            matches!(&actual, PreToolUseDecision::AllowWithUpdate(output) if output.updated_input == Some(expected_map))
        );
    }

    #[test]
    fn test_allow_with_update_complex_nested_input() {
        // updatedInput with nested objects and arrays.
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: r#"{"updatedInput": {"file_path": "/safe/path", "options": {"recursive": true, "depth": 3}, "tags": ["a", "b"]}}"#.to_string(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        let expected_map = serde_json::Map::from_iter([
            ("file_path".to_string(), serde_json::json!("/safe/path")),
            ("options".to_string(), serde_json::json!({"recursive": true, "depth": 3})),
            ("tags".to_string(), serde_json::json!(["a", "b"])),
        ]);
        assert!(
            matches!(&actual, PreToolUseDecision::AllowWithUpdate(output) if output.updated_input == Some(expected_map))
        );
    }

    #[test]
    fn test_block_takes_priority_over_updated_input() {
        // If a hook returns both decision=block AND updatedInput, the block
        // must win because blocking is checked before updatedInput.
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: r#"{"decision": "block", "reason": "nope", "updatedInput": {"command": "echo safe"}}"#.to_string(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        assert!(matches!(actual, PreToolUseDecision::Block(msg) if msg == "nope"));
    }

    #[test]
    fn test_deny_takes_priority_over_updated_input() {
        // permissionDecision=deny should block even if updatedInput is present.
        let results = vec![HookExecutionResult {
            exit_code: Some(0),
            stdout: r#"{"permissionDecision": "deny", "reason": "forbidden", "updatedInput": {"command": "echo safe"}}"#.to_string(),
            stderr: String::new(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        assert!(matches!(actual, PreToolUseDecision::Block(msg) if msg == "forbidden"));
    }

    #[test]
    fn test_exit_code_2_blocks_even_with_updated_input_in_stdout() {
        // Exit code 2 is a hard block regardless of stdout content.
        let results = vec![HookExecutionResult {
            exit_code: Some(2),
            stdout: r#"{"updatedInput": {"command": "echo safe"}}"#.to_string(),
            stderr: "hard block".to_string(),
        }];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        assert!(matches!(actual, PreToolUseDecision::Block(msg) if msg.contains("hard block")));
    }

    #[test]
    fn test_multiple_results_first_update_wins() {
        // When multiple hooks run and the first returns updatedInput, that
        // result is used (iteration stops at first non-Allow decision).
        let results = vec![
            HookExecutionResult {
                exit_code: Some(0),
                stdout: r#"{"updatedInput": {"command": "first"}}"#.to_string(),
                stderr: String::new(),
            },
            HookExecutionResult {
                exit_code: Some(0),
                stdout: r#"{"updatedInput": {"command": "second"}}"#.to_string(),
                stderr: String::new(),
            },
        ];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        let expected_map = serde_json::Map::from_iter([("command".to_string(), serde_json::json!("first"))]);
        assert!(
            matches!(&actual, PreToolUseDecision::AllowWithUpdate(output) if output.updated_input == Some(expected_map))
        );
    }

    #[test]
    fn test_multiple_results_block_before_update() {
        // A block from an earlier hook prevents a later hook's updatedInput
        // from being applied.
        let results = vec![
            HookExecutionResult {
                exit_code: Some(2),
                stdout: String::new(),
                stderr: "blocked first".to_string(),
            },
            HookExecutionResult {
                exit_code: Some(0),
                stdout: r#"{"updatedInput": {"command": "safe"}}"#.to_string(),
                stderr: String::new(),
            },
        ];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        assert!(matches!(actual, PreToolUseDecision::Block(msg) if msg.contains("blocked first")));
    }

    #[test]
    fn test_non_blocking_error_then_update() {
        // A non-blocking error (exit 1) from the first hook is logged but
        // doesn't prevent a subsequent hook from returning updatedInput.
        let results = vec![
            HookExecutionResult {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "warning".to_string(),
            },
            HookExecutionResult {
                exit_code: Some(0),
                stdout: r#"{"updatedInput": {"command": "safe"}}"#.to_string(),
                stderr: String::new(),
            },
        ];
        let actual = UserHookHandler::<NullInfra>::process_pre_tool_use_output(&results);
        let expected_map = serde_json::Map::from_iter([("command".to_string(), serde_json::json!("safe"))]);
        assert!(
            matches!(&actual, PreToolUseDecision::AllowWithUpdate(output) if output.updated_input == Some(expected_map))
        );
    }

    #[tokio::test]
    async fn test_allow_with_update_no_updated_input_preserves_original() {
        // When the hook returns exit 0 with empty stdout (no updatedInput),
        // the original tool call arguments must remain untouched.
        use forge_domain::{
            Agent, EventData, ModelId, ProviderId, ToolCallArguments, ToolCallFull,
            ToolcallStartPayload,
        };

        let json = r#"{"PreToolUse": [{"hooks": [{"type": "command", "command": "echo hi"}]}]}"#;
        let config: UserHookConfig = serde_json::from_str(json).unwrap();
        // NullInfra returns exit 0 + empty stdout => Allow
        let handler = null_handler(config);

        let agent = Agent::new(
            "test-agent",
            ProviderId::from("test-provider".to_string()),
            ModelId::new("test-model"),
        );
        let original_args = ToolCallArguments::from_json(r#"{"command": "ls"}"#);
        let tool_call = ToolCallFull::new("shell").arguments(original_args);
        let mut event = EventData::new(
            agent,
            ModelId::new("test-model"),
            ToolcallStartPayload::new(tool_call),
        );
        let mut conversation = forge_domain::Conversation::generate();

        handler.handle(&mut event, &mut conversation).await.unwrap();

        // Arguments must still be the original value
        let actual_args = event.payload.tool_call.arguments.parse().unwrap();
        let expected_args = serde_json::json!({"command": "ls"});
        assert_eq!(actual_args, expected_args);
    }

    #[tokio::test]
    async fn test_allow_with_update_replaces_unparsed_with_parsed() {
        // Original arguments are Unparsed (raw string from LLM). After
        // AllowWithUpdate, they should become Parsed(Value).
        use forge_domain::{
            Agent, EventData, ModelId, ProviderId, ToolCallArguments, ToolCallFull,
            ToolcallStartPayload,
        };

        let json = r#"{"PreToolUse": [{"hooks": [{"type": "command", "command": "echo hi"}]}]}"#;
        let config: UserHookConfig = serde_json::from_str(json).unwrap();

        #[derive(Clone)]
        struct UpdateInfra2;

        #[async_trait::async_trait]
        impl HookCommandService for UpdateInfra2 {
            async fn execute_command_with_input(
                &self,
                command: String,
                _working_dir: PathBuf,
                _stdin_input: String,
                _env_vars: HashMap<String, String>,
            ) -> anyhow::Result<forge_domain::CommandOutput> {
                Ok(forge_domain::CommandOutput {
                    command,
                    exit_code: Some(0),
                    stdout: r#"{"updatedInput": {"file_path": "/safe/file.txt", "content": "hello"}}"#.to_string(),
                    stderr: String::new(),
                })
            }
        }

        let handler = UserHookHandler::new(
            UpdateInfra2,
            BTreeMap::new(),
            config,
            PathBuf::from("/tmp"),
            "sess-test2".to_string(),
        );

        let agent = Agent::new(
            "test-agent",
            ProviderId::from("test-provider".to_string()),
            ModelId::new("test-model"),
        );
        // Start with Unparsed arguments
        let original_args = ToolCallArguments::from_json(r#"{"file_path": "/etc/passwd", "content": "evil"}"#);
        assert!(matches!(original_args, ToolCallArguments::Unparsed(_)));

        let tool_call = ToolCallFull::new("write").arguments(original_args);
        let mut event = EventData::new(
            agent,
            ModelId::new("test-model"),
            ToolcallStartPayload::new(tool_call),
        );
        let mut conversation = forge_domain::Conversation::generate();

        handler.handle(&mut event, &mut conversation).await.unwrap();

        // After update, arguments should be Parsed
        assert!(matches!(
            event.payload.tool_call.arguments,
            ToolCallArguments::Parsed(_)
        ));
        let actual_args = event.payload.tool_call.arguments.parse().unwrap();
        let expected_args = serde_json::json!({"file_path": "/safe/file.txt", "content": "hello"});
        assert_eq!(actual_args, expected_args);
    }

    #[tokio::test]
    async fn test_block_returns_error_and_preserves_original_args() {
        // When a hook blocks, handle() returns Err and the event arguments
        // remain unchanged.
        use forge_domain::{
            Agent, EventData, ModelId, ProviderId, ToolCallArguments, ToolCallFull,
            ToolcallStartPayload,
        };

        let json = r#"{"PreToolUse": [{"hooks": [{"type": "command", "command": "echo hi"}]}]}"#;
        let config: UserHookConfig = serde_json::from_str(json).unwrap();

        #[derive(Clone)]
        struct BlockInfra;

        #[async_trait::async_trait]
        impl HookCommandService for BlockInfra {
            async fn execute_command_with_input(
                &self,
                command: String,
                _working_dir: PathBuf,
                _stdin_input: String,
                _env_vars: HashMap<String, String>,
            ) -> anyhow::Result<forge_domain::CommandOutput> {
                Ok(forge_domain::CommandOutput {
                    command,
                    exit_code: Some(2),
                    stdout: String::new(),
                    stderr: "dangerous operation".to_string(),
                })
            }
        }

        let handler = UserHookHandler::new(
            BlockInfra,
            BTreeMap::new(),
            config,
            PathBuf::from("/tmp"),
            "sess-block".to_string(),
        );

        let agent = Agent::new(
            "test-agent",
            ProviderId::from("test-provider".to_string()),
            ModelId::new("test-model"),
        );
        let original_args = ToolCallArguments::from_json(r#"{"command": "rm -rf /"}"#);
        let tool_call = ToolCallFull::new("shell").arguments(original_args);
        let mut event = EventData::new(
            agent,
            ModelId::new("test-model"),
            ToolcallStartPayload::new(tool_call),
        );
        let mut conversation = forge_domain::Conversation::generate();

        let result = handler.handle(&mut event, &mut conversation).await;

        // Should be an error
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("blocked by PreToolUse hook"));
        assert!(err_msg.contains("dangerous operation"));

        // Arguments must still be the original value (not modified)
        let actual_args = event.payload.tool_call.arguments.parse().unwrap();
        let expected_args = serde_json::json!({"command": "rm -rf /"});
        assert_eq!(actual_args, expected_args);
    }
}
