use crate::{Context, Transformer};

#[derive(Default)]
pub struct DropReasoningDetails;

impl Transformer for DropReasoningDetails {
    type Value = Context;
    fn transform(&mut self, mut context: Self::Value) -> Self::Value {
        context.messages.iter_mut().for_each(|message| {
            if let crate::ContextMessage::Text(text) = message {
                text.reasoning_details = None;
            }
        });

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
        ContextMessage, ReasoningFull, Role, TextMessage, ToolCallId, ToolName, ToolOutput,
        ToolResult,
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
        }];

        Context::default()
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::User,
                content: "User message with reasoning".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: Some(reasoning_details.clone()),
            }))
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::Assistant,
                content: "Assistant response with reasoning".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: Some(reasoning_details),
            }))
    }

    fn create_context_with_mixed_messages() -> Context {
        let reasoning_details = vec![ReasoningFull {
            text: Some("Complex reasoning process".to_string()),
            signature: None,
        }];

        Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::User,
                content: "User message with reasoning".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: Some(reasoning_details),
            }))
            .add_message(ContextMessage::user("User message without reasoning", None))
            .add_message(ContextMessage::assistant("Assistant response", None, None))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("test_tool"),
                call_id: Some(ToolCallId::new("call_123")),
                output: ToolOutput::text("Tool result".to_string()),
            }])
    }

    #[test]
    fn test_drop_reasoning_details_removes_reasoning() {
        let fixture = create_context_with_reasoning_details();
        let mut transformer = DropReasoningDetails::default();
        let actual = transformer.transform(fixture.clone());

        let snapshot = TransformationSnapshot::new("DropReasoningDetails", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_drop_reasoning_details_preserves_other_fields() {
        let reasoning_details = vec![ReasoningFull {
            text: Some("Important reasoning".to_string()),
            signature: None,
        }];

        let fixture = Context::default().add_message(ContextMessage::Text(TextMessage {
            role: Role::Assistant,
            content: "Assistant message".to_string(),
            tool_calls: None,
            model: Some(crate::ModelId::new("gpt-4")),
            reasoning_details: Some(reasoning_details),
        }));

        let mut transformer = DropReasoningDetails::default();
        let actual = transformer.transform(fixture.clone());

        let snapshot =
            TransformationSnapshot::new("DropReasoningDetails_preserve_fields", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_drop_reasoning_details_mixed_message_types() {
        let fixture = create_context_with_mixed_messages();
        let mut transformer = DropReasoningDetails::default();
        let actual = transformer.transform(fixture.clone());

        let snapshot =
            TransformationSnapshot::new("DropReasoningDetails_mixed_messages", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_drop_reasoning_details_already_none() {
        let fixture = Context::default()
            .add_message(ContextMessage::user("User message", None))
            .add_message(ContextMessage::assistant("Assistant message", None, None))
            .add_message(ContextMessage::system("System message"));

        let mut transformer = DropReasoningDetails::default();
        let actual = transformer.transform(fixture.clone());
        let expected = fixture;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_drop_reasoning_details_preserves_non_text_messages() {
        let reasoning_details =
            vec![ReasoningFull { text: Some("User reasoning".to_string()), signature: None }];

        let fixture = Context::default()
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::User,
                content: "User with reasoning".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: Some(reasoning_details),
            }))
            .add_message(ContextMessage::Image(crate::Image::new_base64(
                "image_data".to_string(),
                "image/png",
            )))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("preserve_tool"),
                call_id: Some(ToolCallId::new("call_preserve")),
                output: ToolOutput::text("Tool output".to_string()),
            }]);

        let mut transformer = DropReasoningDetails::default();
        let actual = transformer.transform(fixture.clone());

        let snapshot =
            TransformationSnapshot::new("DropReasoningDetails_preserve_non_text", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }
}
