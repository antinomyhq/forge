use crate::{Context, Transformer};

/// A transformer that normalizes reasoning details across assistant messages.
///
/// This transformer checks if the first assistant message has reasoning
/// details. If it does, all assistant messages keep their reasoning details.
/// If it doesn't, reasoning details are removed from all assistant messages.
/// This normalizes reasoning behavior across all assistant messages in the
/// conversation.
#[derive(Default)]
pub struct ReasoningNormalizer;

impl Transformer for ReasoningNormalizer {
    type Value = Context;

    fn transform(&mut self, mut context: Self::Value) -> Self::Value {
        // First pass: check if the first assistant message reasoning details
        let first_assistant_has_reasoning = context
            .messages
            .iter()
            .find(|message| message.has_role(crate::Role::Assistant))
            .map(|message| message.has_reasoning_details())
            .unwrap_or(false);

        // Second pass: apply the consistency rule
        if !first_assistant_has_reasoning {
            // Remove reasoning details from all assistant messages
            for message in context.messages.iter_mut() {
                if message.has_role(crate::Role::Assistant)
                    && let crate::ContextMessage::Text(text_msg) = message
                {
                    text_msg.reasoning_details = None;
                }
            }
        }
        // If first_assistant_has_reasoning is true, we keep all reasoning details as-is

        context
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_yaml_snapshot;
    use serde::Serialize;

    use super::*;
    use crate::{ContextMessage, ReasoningFull, Role, TextMessage};

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

    fn create_context_first_assistant_has_reasoning() -> Context {
        let reasoning_details = vec![ReasoningFull {
            text: Some("I need to think about this carefully".to_string()),
            signature: None,
        }];

        Context::default()
            .add_message(ContextMessage::user("User question", None))
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::Assistant,
                content: "First assistant response with reasoning".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: Some(reasoning_details.clone()),
            }))
            .add_message(ContextMessage::user("Follow-up question", None))
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::Assistant,
                content: "Second assistant response with reasoning".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: Some(reasoning_details.clone()),
            }))
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::Assistant,
                content: "Third assistant without reasoning".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: None,
            }))
    }

    fn create_context_first_assistant_no_reasoning() -> Context {
        let reasoning_details = vec![ReasoningFull {
            text: Some("Complex reasoning process".to_string()),
            signature: None,
        }];

        Context::default()
            .add_message(ContextMessage::user("User message", None))
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::Assistant,
                content: "First assistant without reasoning".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: None,
            }))
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::Assistant,
                content: "Second assistant with reasoning".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: Some(reasoning_details.clone()),
            }))
            .add_message(ContextMessage::Text(TextMessage {
                role: Role::Assistant,
                content: "Third assistant with reasoning".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: Some(reasoning_details),
            }))
    }

    #[test]
    fn test_reasoning_normalizer_keeps_all_when_first_has_reasoning() {
        let fixture = create_context_first_assistant_has_reasoning();
        let mut transformer = ReasoningNormalizer::default();
        let actual = transformer.transform(fixture.clone());

        // All reasoning details should be preserved since first assistant has reasoning
        let snapshot =
            TransformationSnapshot::new("ReasoningNormalizer_first_has_reasoning", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_reasoning_normalizer_removes_all_when_first_assistant_message_has_no_reasoning() {
        let fixture = create_context_first_assistant_no_reasoning();
        let mut transformer = ReasoningNormalizer::default();
        let actual = transformer.transform(fixture.clone());

        // All reasoning details should be removed since first assistant has no
        // reasoning
        let snapshot =
            TransformationSnapshot::new("ReasoningNormalizer_first_no_reasoning", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }
}
