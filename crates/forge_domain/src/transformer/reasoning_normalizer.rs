use crate::{Context, Transformer};

/// A transformer that normalizes reasoning details across assistant messages.
///
/// Per Claude's extended thinking docs, when thinking is enabled, assistant
/// messages should include thinking blocks. The docs state: "We recommend you
/// include thinking blocks from previous turns."
///
/// This transformer preserves reasoning on the LAST assistant message that has
/// it, and strips reasoning from all earlier assistant messages to save context
/// space while maintaining the required thinking block for the most recent
/// assistant turn.
///
/// Key behaviors:
/// - If the last assistant message has reasoning: keep only its reasoning,
///   strip from earlier assistants
/// - If the last assistant message has NO reasoning but earlier ones do:
///   preserve the last assistant that HAS reasoning (for API compliance),
///   strip from all others
/// - If no assistant messages have reasoning: nothing to do
#[derive(Default)]
pub struct ReasoningNormalizer;

impl Transformer for ReasoningNormalizer {
    type Value = Context;

    fn transform(&mut self, mut context: Self::Value) -> Self::Value {
        // Find the index of the last assistant message
        let last_assistant_idx = context
            .messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, message)| message.has_role(crate::Role::Assistant))
            .map(|(idx, _)| idx);

        // Check if the last assistant message has reasoning
        let last_assistant_has_reasoning = last_assistant_idx.and_then(|idx| {
            context
                .messages
                .get(idx)
                .map(|message| message.has_reasoning_details())
        });

        // Find the index of the last assistant message that HAS reasoning
        // (may be different from last_assistant_idx if last assistant has no reasoning)
        let last_assistant_with_reasoning_idx = context
            .messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, message)| {
                message.has_role(crate::Role::Assistant) && message.has_reasoning_details()
            })
            .map(|(idx, _)| idx);

        // Apply the normalization rule
        if last_assistant_has_reasoning == Some(true) {
            // Last assistant has reasoning - strip from all previous assistant messages
            // but keep reasoning on the last assistant
            for (idx, message) in context.messages.iter_mut().enumerate() {
                if message.has_role(crate::Role::Assistant)
                    && Some(idx) != last_assistant_idx
                    && let crate::ContextMessage::Text(text_msg) = &mut **message
                {
                    text_msg.reasoning_details = None;
                }
            }
        } else if let Some(preserve_idx) = last_assistant_with_reasoning_idx {
            // Last assistant has NO reasoning, but an earlier assistant does.
            // Preserve reasoning on the last assistant that HAS it (for API compliance),
            // strip from all others.
            // This ensures at least one assistant message has a thinking block.
            for (idx, message) in context.messages.iter_mut().enumerate() {
                if message.has_role(crate::Role::Assistant)
                    && idx != preserve_idx
                    && let crate::ContextMessage::Text(text_msg) = &mut **message
                {
                    text_msg.reasoning_details = None;
                }
            }
        }
        // If no assistant messages have reasoning, nothing to do

        context
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_yaml_snapshot;
    use serde::Serialize;

    use super::*;
    use crate::{ContextMessage, ReasoningConfig, ReasoningFull, Role, TextMessage};

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
            .add_message(ContextMessage::user("User question", None))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "First assistant response with reasoning")
                    .reasoning_details(reasoning_details.clone()),
            ))
            .add_message(ContextMessage::user("Follow-up question", None))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Second assistant response with reasoning")
                    .reasoning_details(reasoning_details.clone()),
            ))
            .add_message(ContextMessage::Text(TextMessage::new(
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
            .add_message(ContextMessage::user("User message", None))
            .add_message(ContextMessage::Text(TextMessage::new(
                Role::Assistant,
                "First assistant without reasoning",
            )))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Second assistant with reasoning")
                    .reasoning_details(reasoning_details.clone()),
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Third assistant with reasoning")
                    .reasoning_details(reasoning_details),
            ))
    }

    #[test]
    fn test_reasoning_normalizer_keeps_all_when_first_has_reasoning() {
        let fixture = create_context_first_assistant_has_reasoning();
        let mut transformer = ReasoningNormalizer;
        let actual = transformer.transform(fixture.clone());

        // When last assistant has no reasoning but earlier ones do,
        // preserve reasoning on the last assistant that HAS it (second assistant)
        // to ensure API compliance with extended thinking requirements
        let snapshot =
            TransformationSnapshot::new("ReasoningNormalizer_first_has_reasoning", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_reasoning_normalizer_removes_all_when_first_assistant_message_has_no_reasoning() {
        let context = create_context_first_assistant_no_reasoning();
        let mut transformer = ReasoningNormalizer;
        let actual = transformer.transform(context.clone());

        // When last assistant (third) has reasoning, keep only its reasoning
        // and strip from all previous assistants
        let snapshot =
            TransformationSnapshot::new("ReasoningNormalizer_first_no_reasoning", context, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_reasoning_normalizer_when_no_assistant_message_present() {
        let context = Context::default()
            .reasoning(ReasoningConfig::default().enabled(true))
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message", None));
        let mut transformer = ReasoningNormalizer;
        let actual = transformer.transform(context.clone());

        // No assistant messages, nothing to normalize
        let snapshot = TransformationSnapshot::new(
            "ReasoningNormalizer_first_no_assistant_message_present",
            context,
            actual,
        );
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_reasoning_normalizer_preserves_last_assistant_after_compaction() {
        // Simulates the scenario after compaction where:
        // 1. Compactor preserved reasoning from the last compacted message
        // 2. Injected it into the first assistant after compaction
        // 3. There are multiple assistant messages in the context
        // Expected: Only the LAST assistant should keep its reasoning
        let preserved_reasoning = vec![ReasoningFull {
            text: Some("Preserved reasoning from compaction".to_string()),
            signature: Some("sig_preserved".to_string()),
            ..Default::default()
        }];

        let other_reasoning = vec![ReasoningFull {
            text: Some("Old reasoning from previous turn".to_string()),
            signature: Some("sig_old".to_string()),
            ..Default::default()
        }];

        let fixture = Context::default()
            .reasoning(ReasoningConfig::default().enabled(true))
            .add_message(ContextMessage::user("Summary after compaction", None))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "First assistant (with injected reasoning)")
                    .reasoning_details(other_reasoning.clone()),
            ))
            .add_message(ContextMessage::user("User question", None))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "Last assistant (current turn)")
                    .reasoning_details(preserved_reasoning.clone()),
            ));

        // Execute
        let mut transformer = ReasoningNormalizer;
        let actual = transformer.transform(fixture.clone());

        // Verify: Only the last assistant should have reasoning
        let last_assistant = actual
            .messages
            .iter()
            .rev()
            .find(|msg| msg.has_role(Role::Assistant))
            .expect("Should have last assistant");

        if let crate::ContextMessage::Text(text) = &**last_assistant {
            assert_eq!(
                text.reasoning_details,
                Some(preserved_reasoning),
                "Last assistant should preserve its reasoning"
            );
        } else {
            panic!("Expected Text message");
        }

        // Verify: First assistant reasoning should be stripped
        let first_assistant = actual
            .messages
            .iter()
            .find(|msg| msg.has_role(Role::Assistant))
            .expect("Should have first assistant");

        if let crate::ContextMessage::Text(text) = &**first_assistant {
            assert_eq!(
                text.reasoning_details, None,
                "First assistant reasoning should be stripped"
            );
        } else {
            panic!("Expected Text message");
        }

        // Verify: Global reasoning config is PRESERVED (not set to None)
        assert!(
            actual.reasoning.is_some(),
            "Reasoning config should be preserved for subsequent turns"
        );
        assert_eq!(
            actual.reasoning.as_ref().unwrap().enabled,
            Some(true),
            "Reasoning should remain enabled for subsequent turns"
        );
    }
}
