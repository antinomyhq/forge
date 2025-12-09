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
            .map(|message| message.has_reasoning_details());

        // Second pass: apply the consistency rule
        if first_assistant_has_reasoning == Some(false) {
            // Remove reasoning details from all assistant messages
            for message in context.messages.iter_mut() {
                if message.has_role(crate::Role::Assistant)
                    && let crate::ContextMessageValue::Text(text_msg) = &mut **message
                {
                    text_msg.reasoning_details = None;
                }
            }

            // Ensure global reasoning config is reset
            context.reasoning = None;
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
    use crate::{ContextMessageValue, ReasoningConfig, ReasoningFull, Role, TextMessage};

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
            ..Default::default()
        }];

        Context::default()
            .reasoning(ReasoningConfig::default().enabled(true))
            .add_message(ContextMessageValue::user("User question", None))
            .add_message(ContextMessageValue::Text(
                TextMessage::new(Role::Assistant, "First assistant response with reasoning")
                    .reasoning_details(reasoning_details.clone()),
            ))
            .add_message(ContextMessageValue::user("Follow-up question", None))
            .add_message(ContextMessageValue::Text(
                TextMessage::new(Role::Assistant, "Second assistant response with reasoning")
                    .reasoning_details(reasoning_details.clone()),
            ))
            .add_message(ContextMessageValue::Text(TextMessage::new(
                Role::Assistant,
                "Third assistant without reasoning",
            )))
    }

    fn create_context_first_assistant_no_reasoning() -> Context {
        let reasoning_details = vec![ReasoningFull {
            text: Some("Complex reasoning process".to_string()),
            signature: None,
            ..Default::default()
        }];

        Context::default()
            .reasoning(ReasoningConfig::default().enabled(true))
            .add_message(ContextMessageValue::user("User message", None))
            .add_message(ContextMessageValue::Text(TextMessage::new(
                Role::Assistant,
                "First assistant without reasoning",
            )))
            .add_message(ContextMessageValue::Text(
                TextMessage::new(Role::Assistant, "Second assistant with reasoning")
                    .reasoning_details(reasoning_details.clone()),
            ))
            .add_message(ContextMessageValue::Text(
                TextMessage::new(Role::Assistant, "Third assistant with reasoning")
                    .reasoning_details(reasoning_details),
            ))
    }

    #[test]
    fn test_reasoning_normalizer_keeps_all_when_first_has_reasoning() {
        let fixture = create_context_first_assistant_has_reasoning();
        let mut transformer = ReasoningNormalizer;
        let actual = transformer.transform(fixture.clone());

        // All reasoning details should be preserved since first assistant has reasoning
        let snapshot =
            TransformationSnapshot::new("ReasoningNormalizer_first_has_reasoning", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_reasoning_normalizer_removes_all_when_first_assistant_message_has_no_reasoning() {
        let context = create_context_first_assistant_no_reasoning();
        let mut transformer = ReasoningNormalizer;
        let actual = transformer.transform(context.clone());

        // All reasoning details should be removed since first assistant has no
        // reasoning
        let snapshot =
            TransformationSnapshot::new("ReasoningNormalizer_first_no_reasoning", context, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_reasoning_normalizer_when_no_assistant_message_present() {
        let context = Context::default()
            .reasoning(ReasoningConfig::default().enabled(true))
            .add_message(ContextMessageValue::system("System message"))
            .add_message(ContextMessageValue::user("User message", None));
        let mut transformer = ReasoningNormalizer;
        let actual = transformer.transform(context.clone());

        // All reasoning details should be removed since first assistant has no
        // reasoning
        let snapshot = TransformationSnapshot::new(
            "ReasoningNormalizer_first_no_assistant_message_present",
            context,
            actual,
        );
        assert_yaml_snapshot!(snapshot);
    }
}
