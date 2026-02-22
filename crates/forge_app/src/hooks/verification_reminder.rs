use forge_domain::{Context, ContextMessage, ToolCatalog};

const VERIFICATION_SKILL_NAME: &str = "verification-specialist";

/// The reminder message injected when the verification-specialist skill has
/// not been called before task completion.
pub const VERIFICATION_REMINDER: &str = "<system-reminder>\nYou have NOT yet invoked the `verification-specialist` skill. You MUST use the `skill` tool to invoke the `verification-specialist` skill to verify your work before marking the task as completed.\n</system-reminder>";

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

#[cfg(test)]
mod tests {
    use forge_domain::{
        Context, ContextMessage, Role, TextMessage, ToolCallArguments, ToolCallFull, ToolCallId,
        ToolName,
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
}
