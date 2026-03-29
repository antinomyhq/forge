use forge_domain::{Context, ContextMessage, ToolCatalog};

const VERIFICATION_SKILL_NAME: &str = "verification-specialist";
const VERIFICATION_COMMAND_TOOL_NAME: &str = "shell";

/// The reminder message injected when the verification-specialist skill has
/// not been called before task completion.
pub const VERIFICATION_REMINDER: &str = "<system-reminder>\nYou have NOT yet invoked the `verification-specialist` skill. You MUST use the `skill` tool to invoke the `verification-specialist` skill and then run the actual verifier command or a runnable smoke test before marking the task as completed. Calling the skill alone is not sufficient.\n</system-reminder>";
pub const VERIFICATION_COMMAND_REMINDER: &str = "<system-reminder>\nYou have invoked the `verification-specialist` skill, but there is still no successful `shell` verification command after that skill call in the transcript. You MUST run the actual verifier command or a runnable smoke test and leave its output in the conversation before marking the task as completed.\n</system-reminder>";

/// Returns true if the `verification-specialist` skill was called anywhere in
/// the given context.
pub fn verification_skill_was_called(context: &Context) -> bool {
    context.messages.iter().any(|msg| {
        if let ContextMessage::Text(text_msg) = &**msg
            && let Some(tool_calls) = &text_msg.tool_calls
        {
            return tool_calls.iter().any(|call| {
                if let Ok(ToolCatalog::Skill(skill)) = ToolCatalog::try_from(call.clone()) {
                    skill.name == VERIFICATION_SKILL_NAME
                } else {
                    false
                }
            });
        }
        false
    })
}

/// Returns true if a successful `shell` verification command appears after the
/// most recent `verification-specialist` skill call.
pub fn verification_command_was_run_after_skill(context: &Context) -> bool {
    let mut seen_latest_skill = false;
    let mut verification_command_succeeded = false;

    for msg in &context.messages {
        match &**msg {
            ContextMessage::Text(text_msg) => {
                let Some(tool_calls) = &text_msg.tool_calls else {
                    continue;
                };

                for call in tool_calls {
                    let is_verification_skill = ToolCatalog::try_from(call.clone())
                        .ok()
                        .and_then(|tool| match tool {
                            ToolCatalog::Skill(skill) => {
                                Some(skill.name == VERIFICATION_SKILL_NAME)
                            }
                            _ => None,
                        })
                        .unwrap_or(false);

                    if is_verification_skill {
                        seen_latest_skill = true;
                        verification_command_succeeded = false;
                        continue;
                    }

                    if seen_latest_skill && call.name.as_str() == VERIFICATION_COMMAND_TOOL_NAME {
                        verification_command_succeeded = false;
                    }
                }
            }
            ContextMessage::Tool(result) => {
                if seen_latest_skill
                    && result.name.as_str() == VERIFICATION_COMMAND_TOOL_NAME
                    && !result.is_error()
                {
                    verification_command_succeeded = true;
                }
            }
            _ => {}
        }
    }

    verification_command_succeeded
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        Context, ContextMessage, Role, TextMessage, ToolCallArguments, ToolCallFull, ToolCallId,
        ToolName, ToolOutput, ToolResult,
    };

    use super::*;

    fn skill_tool_call(skill_name: &str) -> ToolCallFull {
        ToolCallFull {
            name: ToolName::new("skill"),
            call_id: Some(ToolCallId::new("call_1")),
            arguments: ToolCallArguments::from_json(&format!(r#"{{"name":"{}"}}"#, skill_name)),
            thought_signature: None,
        }
    }

    fn context_with_skill_call(skill_name: &str) -> Context {
        Context::default().add_message(ContextMessage::Text(
            TextMessage::new(Role::Assistant, "Invoking skill")
                .tool_calls(vec![skill_tool_call(skill_name)]),
        ))
    }

    fn shell_tool_call() -> ToolCallFull {
        ToolCallFull {
            name: ToolName::new(VERIFICATION_COMMAND_TOOL_NAME),
            call_id: Some(ToolCallId::new("call_shell")),
            arguments: ToolCallArguments::from_json(r#"{"command":"pytest"}"#),
            thought_signature: None,
        }
    }

    fn shell_tool_result(is_error: bool) -> ToolResult {
        let output = if is_error {
            ToolOutput::text("failed").is_error(true)
        } else {
            ToolOutput::text("passed")
        };
        ToolResult::new(VERIFICATION_COMMAND_TOOL_NAME)
            .call_id(ToolCallId::new("call_shell"))
            .output(Ok(output))
    }

    fn context_without_skill_call() -> Context {
        Context::default().add_message(ContextMessage::user("Hello", None))
    }

    #[test]
    fn test_returns_true_when_verification_specialist_called() {
        let context = context_with_skill_call(VERIFICATION_SKILL_NAME);
        assert!(verification_skill_was_called(&context));
    }

    #[test]
    fn test_returns_false_when_no_skill_called() {
        let context = context_without_skill_call();
        assert!(!verification_skill_was_called(&context));
    }

    #[test]
    fn test_returns_false_when_different_skill_called() {
        let context = context_with_skill_call("create-pr-description");
        assert!(!verification_skill_was_called(&context));
    }

    #[test]
    fn test_returns_false_for_empty_context() {
        let context = Context::default();
        assert!(!verification_skill_was_called(&context));
    }

    #[test]
    fn test_returns_true_when_skill_called_among_many_messages() {
        let non_skill_call = ToolCallFull {
            name: ToolName::new("shell"),
            call_id: None,
            arguments: ToolCallArguments::from_json(r#"{"command":"ls"}"#),
            thought_signature: None,
        };
        let context = Context::default()
            .add_message(ContextMessage::user("task", None))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Running shell").tool_calls(vec![non_skill_call]),
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Invoking skill")
                    .tool_calls(vec![skill_tool_call(VERIFICATION_SKILL_NAME)]),
            ));
        assert!(verification_skill_was_called(&context));
    }

    #[test]
    fn test_verification_command_returns_false_without_skill() {
        let context = Context::default()
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Running shell")
                    .tool_calls(vec![shell_tool_call()]),
            ))
            .add_tool_results(vec![shell_tool_result(false)]);
        assert!(!verification_command_was_run_after_skill(&context));
    }

    #[test]
    fn test_verification_command_returns_true_after_skill() {
        let context = Context::default()
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Verify").tool_calls(vec![
                    skill_tool_call(VERIFICATION_SKILL_NAME),
                    shell_tool_call(),
                ]),
            ))
            .add_tool_results(vec![shell_tool_result(false)]);
        assert!(verification_command_was_run_after_skill(&context));
    }

    #[test]
    fn test_verification_command_returns_false_for_failed_shell() {
        let context = Context::default()
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Verify").tool_calls(vec![
                    skill_tool_call(VERIFICATION_SKILL_NAME),
                    shell_tool_call(),
                ]),
            ))
            .add_tool_results(vec![shell_tool_result(true)]);
        assert!(!verification_command_was_run_after_skill(&context));
    }

    #[test]
    fn test_verification_command_uses_latest_skill_call() {
        let context = Context::default()
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Verify once").tool_calls(vec![
                    skill_tool_call(VERIFICATION_SKILL_NAME),
                    shell_tool_call(),
                ]),
            ))
            .add_tool_results(vec![shell_tool_result(false)])
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Verify again")
                    .tool_calls(vec![skill_tool_call(VERIFICATION_SKILL_NAME)]),
            ));
        assert!(!verification_command_was_run_after_skill(&context));
    }
}
