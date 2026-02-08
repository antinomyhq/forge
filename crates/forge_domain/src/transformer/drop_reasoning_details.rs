use crate::{Context, ModelId, Transformer};

#[derive(Default)]
pub struct DropReasoningDetails;

impl Transformer for DropReasoningDetails {
    type Value = Context;
    fn transform(&mut self, mut context: Self::Value) -> Self::Value {
        context.messages.iter_mut().for_each(|message| {
            if let crate::ContextMessage::Text(text) = &mut **message {
                text.reasoning_details = None;
            }
        });

        // Drop reasoning configuration
        context.reasoning = None;

        context
    }
}

/// Transformer that drops reasoning_details from messages created before the
/// current model was used.
///
/// This is specifically designed to handle model switching scenarios on
/// OpenRouter. When a user switches from model A to model B, this transformer
/// ensures that only reasoning from the current model session is preserved.
///
/// # How It Works
///
/// Since the `model` field only exists on user messages (not assistant/tool
/// messages), we use user messages as "markers" to detect when the model
/// switch occurred:
///
/// 1. Searches backward through messages to find the first user message with
///    the current model
/// 2. This marks the "model switch point"
/// 3. Drops reasoning_details from all messages BEFORE that point
/// 4. Preserves reasoning_details from all messages AT/AFTER that point
///
/// # Example Scenarios
///
/// ## Scenario 1: ZAI → Anthropic
/// ```text
/// Message 0: User "hello" (model="z-ai/glm-4.7")
/// Message 1: Assistant with reasoning (from ZAI)
/// Message 2: User "switch" (model="anthropic/claude") ← SWITCH POINT
/// Message 3: Assistant with reasoning (from Anthropic)
///
/// Result: Drop reasoning from messages 0-1, keep 2-3
/// ```
///
/// ## Scenario 2: ZAI → Anthropic → ZAI
/// ```text
/// Message 0: User "hello" (model="z-ai/glm-4.7")
/// Message 1: Assistant with reasoning (from first ZAI session)
/// Message 2: User "switch to claude" (model="anthropic/claude")
/// Message 3: Assistant with reasoning (from Anthropic)
/// Message 4: User "back to zai" (model="z-ai/glm-4.7") ← SWITCH POINT
/// Message 5: Assistant with reasoning (from second ZAI session)
///
/// Result: Drop reasoning from messages 2-3 (Anthropic), keep 1 and 5 (both ZAI)
/// ```
#[derive(Debug)]
pub struct DropReasoningDetailsFromOtherModels {
    current_model: ModelId,
}

impl DropReasoningDetailsFromOtherModels {
    /// Creates a new transformer with the current model ID
    pub fn new(current_model: ModelId) -> Self {
        Self { current_model }
    }
}

impl Transformer for DropReasoningDetailsFromOtherModels {
    type Value = Context;
    fn transform(&mut self, mut context: Self::Value) -> Self::Value {
        // Find the first user message with the current model (searching from end)
        // This marks the "model switch point"
        let switch_point = context
            .messages
            .iter()
            .enumerate()
            .rev()
            .find_map(|(idx, msg)| {
                if let crate::ContextMessage::Text(text) = &**msg
                    && text.role == crate::Role::User
                        && let Some(message_model) = &text.model
                            && message_model == &self.current_model {
                                return Some(idx);
                            }
                None
            });

        // Drop reasoning from all messages before the switch point
        if let Some(switch_idx) = switch_point {
            for (idx, message) in context.messages.iter_mut().enumerate() {
                if idx < switch_idx
                    && let crate::ContextMessage::Text(text) = &mut **message {
                        text.reasoning_details = None;
                    }
            }
        }

        context
    }
}
#[cfg(test)]
mod tests {
    use insta::assert_yaml_snapshot;
    use pretty_assertions::assert_eq;
    use serde::Serialize;

    use super::*;
    use crate::{
        ContextMessage, ReasoningConfig, ReasoningFull, Role, TextMessage, ToolCallId, ToolName,
        ToolOutput, ToolResult,
    };

    #[derive(Serialize)]
    struct TransformationSnapshot {
        transformation: String,
        before: Context,
        after: Context,
    }

    impl TransformationSnapshot {
        fn new(transformation: &str, before: Context, after: Context) -> Self {
            Self { transformation: transformation.to_string(), before, after }
        }
    }

    fn create_context_with_reasoning_details() -> Context {
        let reasoning_details = vec![ReasoningFull {
            text: Some("I need to think about this".to_string()),
            signature: None,
            ..Default::default()
        }];

        Context::default()
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::User, "User message with reasoning")
                    .reasoning_details(reasoning_details.clone()),
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Assistant response with reasoning")
                    .reasoning_details(reasoning_details),
            ))
    }

    fn create_context_with_mixed_messages() -> Context {
        let reasoning_details = vec![ReasoningFull {
            text: Some("Complex reasoning process".to_string()),
            signature: None,
            ..Default::default()
        }];

        Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::User, "User message with reasoning")
                    .reasoning_details(reasoning_details),
            ))
            .add_message(ContextMessage::user("User message without reasoning", None))
            .add_message(ContextMessage::assistant(
                "Assistant response",
                None,
                None,
                None,
            ))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("test_tool"),
                call_id: Some(ToolCallId::new("call_123")),
                output: ToolOutput::text("Tool result".to_string()),
            }])
    }

    #[test]
    fn test_drop_reasoning_details_removes_reasoning() {
        let fixture = create_context_with_reasoning_details();
        let mut transformer = DropReasoningDetails;
        let actual = transformer.transform(fixture.clone());

        let snapshot = TransformationSnapshot::new("DropReasoningDetails", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_drop_reasoning_details_preserves_other_fields() {
        let reasoning_details = vec![ReasoningFull {
            text: Some("Important reasoning".to_string()),
            signature: None,
            ..Default::default()
        }];

        let fixture = Context::default().add_message(ContextMessage::Text(
            TextMessage::new(Role::Assistant, "Assistant message")
                .model(crate::ModelId::new("gpt-4"))
                .reasoning_details(reasoning_details),
        ));

        let mut transformer = DropReasoningDetails;
        let actual = transformer.transform(fixture.clone());

        let snapshot =
            TransformationSnapshot::new("DropReasoningDetails_preserve_fields", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_drop_reasoning_details_mixed_message_types() {
        let fixture = create_context_with_mixed_messages();
        let mut transformer = DropReasoningDetails;
        let actual = transformer.transform(fixture.clone());

        let snapshot =
            TransformationSnapshot::new("DropReasoningDetails_mixed_messages", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_drop_reasoning_details_already_none() {
        let fixture = Context::default()
            .add_message(ContextMessage::user("User message", None))
            .add_message(ContextMessage::assistant(
                "Assistant message",
                None,
                None,
                None,
            ))
            .add_message(ContextMessage::system("System message"));

        let mut transformer = DropReasoningDetails;
        let actual = transformer.transform(fixture.clone());
        let expected = fixture;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_drop_reasoning_details_preserves_non_text_messages() {
        let reasoning_details = vec![ReasoningFull {
            text: Some("User reasoning".to_string()),
            signature: None,
            ..Default::default()
        }];

        let fixture = Context::default()
            .reasoning(ReasoningConfig::default().enabled(true))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::User, "User with reasoning")
                    .reasoning_details(reasoning_details),
            ))
            .add_message(ContextMessage::Image(crate::Image::new_base64(
                "image_data".to_string(),
                "image/png",
            )))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("preserve_tool"),
                call_id: Some(ToolCallId::new("call_preserve")),
                output: ToolOutput::text("Tool output".to_string()),
            }]);

        let mut transformer = DropReasoningDetails;
        let actual = transformer.transform(fixture.clone());

        let snapshot =
            TransformationSnapshot::new("DropReasoningDetails_preserve_non_text", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_drop_reasoning_from_other_models_only() {
        let old_model = crate::ModelId::new("z-ai/glm-4.7");
        let new_model = crate::ModelId::new("anthropic/claude-sonnet-4.5");

        let reasoning_from_old = vec![ReasoningFull {
            text: Some("Old model reasoning".to_string()),
            signature: None,
            ..Default::default()
        }];

        let reasoning_from_new = vec![ReasoningFull {
            text: Some("New model reasoning".to_string()),
            signature: None,
            ..Default::default()
        }];

        // Simulate a conversation with model switch:
        // 1. User message with old model
        // 2. Assistant response (has reasoning from old model)
        // 3. User message with new model (switch point)
        // 4. Assistant response (has reasoning from new model)
        let fixture = Context::default()
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::User, "First user message").model(old_model.clone()),
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Old model response")
                    .reasoning_details(reasoning_from_old),
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::User, "Second user message (model switch)")
                    .model(new_model.clone()),
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "New model response")
                    .reasoning_details(reasoning_from_new),
            ));

        let mut transformer = super::DropReasoningDetailsFromOtherModels::new(new_model);
        let actual = transformer.transform(fixture.clone());

        // Message 1 (assistant from old model) should have reasoning_details removed
        if let crate::ContextMessage::Text(text) = &*actual.messages[1] {
            assert!(
                text.reasoning_details.is_none(),
                "Old model reasoning should be dropped (before switch point)"
            );
        }

        // Message 3 (assistant from new model) should keep reasoning_details
        if let crate::ContextMessage::Text(text) = &*actual.messages[3] {
            assert!(
                text.reasoning_details.is_some(),
                "New model reasoning should be preserved (after switch point)"
            );
        }
    }

    #[test]
    fn test_drop_reasoning_from_other_models_all_same() {
        let model = crate::ModelId::new("anthropic/claude-sonnet-4.5");

        let reasoning = vec![ReasoningFull {
            text: Some("Some reasoning".to_string()),
            signature: None,
            ..Default::default()
        }];

        let fixture = Context::default()
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::User, "User 1").model(model.clone()),
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Response 1")
                    .reasoning_details(reasoning.clone()),
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::User, "User 2").model(model.clone()),
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Response 2").reasoning_details(reasoning),
            ));

        let mut transformer = super::DropReasoningDetailsFromOtherModels::new(model);
        let actual = transformer.transform(fixture);

        // The transformer finds the LAST user message with current model (idx 2)
        // and drops reasoning from all messages BEFORE that point
        // So Response 1 (idx 1) gets dropped, but Response 2 (idx 3) is preserved
        if let crate::ContextMessage::Text(text) = &*actual.messages[1] {
            assert!(
                text.reasoning_details.is_none(),
                "Reasoning before last user message should be dropped"
            );
        }
        if let crate::ContextMessage::Text(text) = &*actual.messages[3] {
            assert!(
                text.reasoning_details.is_some(),
                "Reasoning after last user message should be preserved"
            );
        }
    }
}
