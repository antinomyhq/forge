use crate::{Context, Role, Transformer};

/// A transformer that normalizes reasoning details across assistant messages.
///
/// Per Claude's extended thinking docs, thinking blocks from previous turns are
/// stripped to save context space, but the LAST assistant turn's thinking must
/// be preserved for reasoning continuity (especially during tool use).
///
/// Kimi's coding endpoint is stricter about replayed assistant tool-call turns,
/// so the Kimi replay mode preserves reasoning on assistant messages that carry
/// tool calls in addition to the most recent assistant reasoning turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ReasoningNormalizationMode {
    #[default]
    Standard,
    KimiReplay,
}

#[derive(Default)]
pub struct ReasoningNormalizer {
    mode: ReasoningNormalizationMode,
}

impl ReasoningNormalizer {
    pub fn kimi_replay() -> Self {
        Self { mode: ReasoningNormalizationMode::KimiReplay }
    }

    fn should_preserve_reasoning(
        &self,
        idx: usize,
        text_message: &crate::TextMessage,
        last_assistant_idx: Option<usize>,
        preserve_last_reasoning: bool,
    ) -> bool {
        match self.mode {
            ReasoningNormalizationMode::Standard => {
                preserve_last_reasoning && Some(idx) == last_assistant_idx
            }
            ReasoningNormalizationMode::KimiReplay => {
                text_message.tool_calls.is_some()
                    || (preserve_last_reasoning && Some(idx) == last_assistant_idx)
            }
        }
    }
}

impl Transformer for ReasoningNormalizer {
    type Value = Context;

    fn transform(&mut self, mut context: Self::Value) -> Self::Value {
        // Find the index of the last assistant message
        let last_assistant_idx = context
            .messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, message)| message.has_role(Role::Assistant))
            .map(|(idx, _)| idx);

        // Check if the last assistant message has reasoning
        let preserve_last_reasoning = last_assistant_idx.and_then(|idx| {
            context
                .messages
                .get(idx)
                .map(|message| message.has_reasoning_details())
        }) == Some(true);

        for (idx, message) in context.messages.iter_mut().enumerate() {
            if let crate::ContextMessage::Text(text_msg) = &mut **message {
                if text_msg.role != Role::Assistant {
                    continue;
                }

                if !self.should_preserve_reasoning(
                    idx,
                    text_msg,
                    last_assistant_idx,
                    preserve_last_reasoning,
                ) {
                    text_msg.reasoning_details = None;
                }
            }
        }

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

    fn create_context_assistant_reasoning_history() -> Context {
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

    fn create_context_last_assistant_has_reasoning() -> Context {
        let reasoning_details = vec![ReasoningFull {
            text: Some("Complex reasoning process".to_string()),
            signature: None,
            ..Default::default()
        }];

        Context::default()
            .reasoning(ReasoningConfig::default().enabled(true))
            .add_message(ContextMessage::user("User message", None))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::Assistant, "First assistant with reasoning")
                    .reasoning_details(reasoning_details.clone()),
            ))
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
    fn test_reasoning_normalizer_drops_reasoning_when_last_assistant_has_none() {
        let fixture = create_context_assistant_reasoning_history();
        let mut transformer = ReasoningNormalizer::default();
        let actual = transformer.transform(fixture.clone());

        let snapshot = TransformationSnapshot::new(
            "ReasoningNormalizer_last_assistant_has_no_reasoning",
            fixture,
            actual,
        );
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_reasoning_normalizer_preserves_only_last_assistant_reasoning() {
        let context = create_context_last_assistant_has_reasoning();
        let mut transformer = ReasoningNormalizer::default();
        let actual = transformer.transform(context.clone());

        let assistant_messages = actual
            .messages
            .iter()
            .filter_map(|message| match &**message {
                crate::ContextMessage::Text(text) if text.role == Role::Assistant => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            assistant_messages.len(),
            3,
            "Expected three assistant messages"
        );
        assert_eq!(assistant_messages[0].reasoning_details, None);
        assert_eq!(assistant_messages[1].reasoning_details, None);
        assert!(assistant_messages[2].reasoning_details.is_some());

        let snapshot = TransformationSnapshot::new(
            "ReasoningNormalizer_preserves_only_last_assistant_reasoning",
            context,
            actual,
        );
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_reasoning_normalizer_when_no_assistant_message_present() {
        let context = Context::default()
            .reasoning(ReasoningConfig::default().enabled(true))
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message", None));
        let mut transformer = ReasoningNormalizer::default();
        let actual = transformer.transform(context.clone());

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
        let mut transformer = ReasoningNormalizer::default();
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

    #[test]
    fn test_kimi_replay_preserves_tool_call_reasoning_and_last_assistant_reasoning() {
        let replay_reasoning = vec![ReasoningFull {
            text: Some("Reasoning attached to replayed tool call".to_string()),
            signature: Some("sig_tool".to_string()),
            ..Default::default()
        }];
        let last_reasoning = vec![ReasoningFull {
            text: Some("Reasoning attached to the latest assistant turn".to_string()),
            signature: Some("sig_last".to_string()),
            ..Default::default()
        }];

        let tool_call = crate::ToolCallFull::new("shell")
            .call_id(crate::ToolCallId::new("call_kimi_1"))
            .arguments(crate::ToolCallArguments::from(
                serde_json::json!({"command": "pwd"}),
            ));

        let fixture = Context::default()
            .reasoning(ReasoningConfig::default().enabled(true))
            .add_message(ContextMessage::user("Initial request", None))
            .add_message(ContextMessage::assistant(
                "Let me inspect the workspace",
                None,
                Some(replay_reasoning.clone()),
                Some(vec![tool_call]),
            ))
            .add_tool_results(vec![
                crate::ToolResult::new("shell")
                    .call_id(Some(crate::ToolCallId::new("call_kimi_1")))
                    .output(Ok(crate::ToolOutput::text("/workspace".to_string()))),
            ])
            .add_message(ContextMessage::user("Continue", None))
            .add_message(ContextMessage::assistant(
                "Here is the result",
                None,
                Some(last_reasoning.clone()),
                None,
            ));

        let mut transformer = ReasoningNormalizer::kimi_replay();
        let actual = transformer.transform(fixture.clone());

        let assistant_messages = actual
            .messages
            .iter()
            .filter_map(|message| match &**message {
                crate::ContextMessage::Text(text) if text.role == Role::Assistant => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            assistant_messages.len(),
            2,
            "Expected two assistant messages"
        );
        assert_eq!(
            assistant_messages[0].reasoning_details,
            Some(replay_reasoning)
        );
        assert_eq!(
            assistant_messages[1].reasoning_details,
            Some(last_reasoning)
        );

        let snapshot = TransformationSnapshot::new(
            "ReasoningNormalizer_kimi_replay_preserves_tool_call_reasoning",
            fixture,
            actual,
        );
        assert_yaml_snapshot!(snapshot);
    }
}
