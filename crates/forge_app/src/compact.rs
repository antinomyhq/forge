use forge_domain::{
    Compact, CompactionStrategy, Context, ContextMessage, ContextSummary, Environment, Transformer,
};
use tracing::info;

use crate::TemplateEngine;
use crate::transformers::SummaryTransformer;

/// A service dedicated to handling context compaction.
pub struct Compactor {
    compact: Compact,
    environment: Environment,
}

impl Compactor {
    pub fn new(compact: Compact, environment: Environment) -> Self {
        Self { compact, environment }
    }

    /// Applies the standard compaction transformer pipeline to a context
    /// summary.
    ///
    /// This pipeline uses the `Compaction` transformer which:
    /// 1. Drops system role messages
    /// 2. Deduplicates consecutive user messages
    /// 3. Trims context by keeping only the last operation per file path
    /// 4. Deduplicates consecutive assistant content blocks
    /// 5. Strips working directory prefix from file paths
    ///
    /// # Arguments
    ///
    /// * `context_summary` - The context summary to transform
    fn transform(&self, context_summary: ContextSummary) -> ContextSummary {
        SummaryTransformer::new(&self.environment.cwd).transform(context_summary)
    }
}

impl Compactor {
    /// Apply compaction to the context if requested.
    pub fn compact(&self, context: Context, max: bool) -> anyhow::Result<Context> {
        let eviction = CompactionStrategy::evict(self.compact.eviction_window);
        let retention = CompactionStrategy::retain(self.compact.retention_window);

        let strategy = if max {
            // TODO: Consider using `eviction.max(retention)`
            retention
        } else {
            eviction.min(retention)
        };

        match strategy.eviction_range(&context) {
            Some(sequence) => self.compress_single_sequence(context, sequence),
            None => Ok(context),
        }
    }

    /// Compress a single identified sequence of assistant messages.
    fn compress_single_sequence(
        &self,
        mut context: Context,
        sequence: (usize, usize),
    ) -> anyhow::Result<Context> {
        let (start, end) = sequence;

        // The sequence from the original message that needs to be compacted
        // Filter out droppable messages (e.g., attachments) from compaction
        let compaction_sequence: Vec<ContextMessage> = context.messages[start..=end]
            .iter()
            .filter(|msg| !msg.is_droppable())
            .cloned()
            .collect();

        // Create a temporary context for the sequence to generate summary
        let sequence_context = Context::default().messages(compaction_sequence.clone());

        // Generate context summary with tool call information
        let context_summary = ContextSummary::from(&sequence_context);

        // Apply transformers to reduce redundant operations and clean up
        let context_summary = self.transform(context_summary);

        info!(
            sequence_start = sequence.0,
            sequence_end = sequence.1,
            sequence_length = compaction_sequence.len(),
            "Created context compaction summary"
        );

        let summary = TemplateEngine::default().render(
            "forge-partial-summary-frame.md",
            &serde_json::json!({"messages": context_summary.messages}),
        )?;

        // Extended thinking reasoning chain preservation
        //
        // Extended thinking requires the first assistant message to have
        // reasoning_details for subsequent messages to maintain reasoning
        // chains. After compaction, this consistency can break if the first
        // remaining assistant lacks reasoning.
        //
        // Solution: Extract the LAST reasoning from compacted messages and inject it
        // into the first assistant message after compaction. This preserves
        // chain continuity while preventing exponential accumulation across
        // multiple compactions.
        //
        // Example: [U, A+r, U, A+r, U, A] → compact → [U-summary, A+r, U, A]
        //                                                          └─from last
        // compacted
        let reasoning_details = compaction_sequence
            .iter()
            .rev() // Get LAST reasoning (most recent)
            .find_map(|msg| match msg {
                ContextMessage::Text(text) => text
                    .reasoning_details
                    .as_ref()
                    .filter(|rd| !rd.is_empty())
                    .cloned(),
                _ => None,
            });

        // Replace the range with the summary
        context.messages.splice(
            start..=end,
            std::iter::once(ContextMessage::user(summary, None)),
        );

        // Remove all droppable messages from the context
        context.messages.retain(|msg| !msg.is_droppable());

        // Inject preserved reasoning into first assistant message (if empty)
        if let Some(reasoning) = reasoning_details
            && let Some(ContextMessage::Text(msg)) = context
                .messages
                .iter_mut()
                .find(|msg| msg.has_role(forge_domain::Role::Assistant))
            && msg
                .reasoning_details
                .as_ref()
                .is_none_or(|rd| rd.is_empty())
        {
            msg.reasoning_details = Some(reasoning);
        }

        // Clear usage field so token_count() recalculates based on new messages
        context.usage = None;

        Ok(context)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    fn test_environment() -> Environment {
        use fake::{Fake, Faker};
        let env: Environment = Faker.fake();
        env.cwd(std::path::PathBuf::from("/test/working/dir"))
    }

    #[test]
    fn test_compress_single_sequence_preserves_only_last_reasoning() {
        use forge_domain::ReasoningFull;

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        let first_reasoning = vec![ReasoningFull {
            text: Some("First thought".to_string()),
            signature: Some("sig1".to_string()),
            ..Default::default()
        }];

        let last_reasoning = vec![ReasoningFull {
            text: Some("Last thought".to_string()),
            signature: Some("sig2".to_string()),
            ..Default::default()
        }];

        let context = Context::default()
            .add_message(ContextMessage::user("M1", None))
            .add_message(ContextMessage::assistant(
                "R1",
                Some(first_reasoning.clone()),
                None,
            ))
            .add_message(ContextMessage::user("M2", None))
            .add_message(ContextMessage::assistant(
                "R2",
                Some(last_reasoning.clone()),
                None,
            ))
            .add_message(ContextMessage::user("M3", None))
            .add_message(ContextMessage::assistant("R3", None, None));

        let actual = compactor.compress_single_sequence(context, (0, 3)).unwrap();

        // Verify only LAST reasoning_details were preserved
        let assistant_msg = actual
            .messages
            .iter()
            .find(|msg| msg.has_role(forge_domain::Role::Assistant))
            .expect("Should have an assistant message");

        if let ContextMessage::Text(text_msg) = assistant_msg {
            assert_eq!(
                text_msg.reasoning_details.as_ref(),
                Some(&last_reasoning),
                "Should preserve only the last reasoning, not the first"
            );
        } else {
            panic!("Expected TextMessage");
        }
    }

    #[test]
    fn test_compress_single_sequence_no_reasoning_accumulation() {
        use forge_domain::ReasoningFull;

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        let reasoning = vec![ReasoningFull {
            text: Some("Original thought".to_string()),
            signature: Some("sig1".to_string()),
            ..Default::default()
        }];

        // First compaction
        let context = Context::default()
            .add_message(ContextMessage::user("M1", None))
            .add_message(ContextMessage::assistant(
                "R1",
                Some(reasoning.clone()),
                None,
            ))
            .add_message(ContextMessage::user("M2", None))
            .add_message(ContextMessage::assistant("R2", None, None));

        let context = compactor.compress_single_sequence(context, (0, 1)).unwrap();

        // Verify first assistant has the reasoning
        let first_assistant = context
            .messages
            .iter()
            .find(|msg| msg.has_role(forge_domain::Role::Assistant))
            .unwrap();

        if let ContextMessage::Text(text_msg) = first_assistant {
            assert_eq!(text_msg.reasoning_details.as_ref().unwrap().len(), 1);
        }

        // Second compaction - add more messages
        let context = context
            .add_message(ContextMessage::user("M3", None))
            .add_message(ContextMessage::assistant("R3", None, None));

        let context = compactor.compress_single_sequence(context, (0, 2)).unwrap();

        // Verify reasoning didn't accumulate - should still be just 1 reasoning block
        let first_assistant = context
            .messages
            .iter()
            .find(|msg| msg.has_role(forge_domain::Role::Assistant))
            .unwrap();

        if let ContextMessage::Text(text_msg) = first_assistant {
            assert_eq!(
                text_msg.reasoning_details.as_ref().unwrap().len(),
                1,
                "Reasoning should not accumulate across compactions"
            );
        }
    }

    #[test]
    fn test_compress_single_sequence_filters_empty_reasoning() {
        use forge_domain::ReasoningFull;

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        let non_empty_reasoning = vec![ReasoningFull {
            text: Some("Valid thought".to_string()),
            signature: Some("sig1".to_string()),
            ..Default::default()
        }];

        // Most recent message in range has empty reasoning, earlier has non-empty
        let context = Context::default()
            .add_message(ContextMessage::user("M1", None))
            .add_message(ContextMessage::assistant(
                "R1",
                Some(non_empty_reasoning.clone()),
                None,
            ))
            .add_message(ContextMessage::user("M2", None))
            .add_message(ContextMessage::assistant("R2", Some(vec![]), None)) // Empty - most recent in range
            .add_message(ContextMessage::user("M3", None))
            .add_message(ContextMessage::assistant("R3", None, None)); // Outside range

        let actual = compactor.compress_single_sequence(context, (0, 3)).unwrap();

        // After compression: [U-summary, U3, A3]
        // The reasoning from R1 (non-empty) should be injected into A3
        let assistant_msg = actual
            .messages
            .iter()
            .find(|msg| msg.has_role(forge_domain::Role::Assistant))
            .expect("Should have an assistant message");

        if let ContextMessage::Text(text_msg) = assistant_msg {
            assert_eq!(
                text_msg.reasoning_details.as_ref(),
                Some(&non_empty_reasoning),
                "Should skip most recent empty reasoning and preserve earlier non-empty"
            );
        } else {
            panic!("Expected TextMessage");
        }
    }

    fn render_template(data: &serde_json::Value) -> String {
        TemplateEngine::default()
            .render("forge-partial-summary-frame.md", data)
            .unwrap()
    }

    #[test]
    fn test_template_engine_renders_summary_frame() {
        use forge_domain::{ContextSummary, Role, SummaryBlock, SummaryMessage, SummaryToolCall};

        // Create test data with various tool calls and text content
        let messages = vec![
            SummaryBlock::new(
                Role::User,
                vec![SummaryMessage::content("Please read the config file")],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![
                    SummaryToolCall::read("config.toml")
                        .id("call_1")
                        .is_success(false)
                        .into(),
                ],
            ),
            SummaryBlock::new(
                Role::User,
                vec![SummaryMessage::content("Now update the version number")],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![SummaryToolCall::update("Cargo.toml").id("call_2").into()],
            ),
            SummaryBlock::new(
                Role::User,
                vec![SummaryMessage::content("Search for TODO comments")],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![
                    SummaryToolCall::search("TODO")
                        .id("call_3")
                        .is_success(false)
                        .into(),
                ],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![
                    SummaryToolCall::codebase_search(
                        vec![forge_domain::SearchQuery::new(
                            "authentication logic",
                            "Find authentication implementation",
                        )],
                        Some(".rs".to_string()),
                    )
                    .id("call_4")
                    .is_success(false)
                    .into(),
                ],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![
                    SummaryToolCall::shell("cargo test")
                        .id("call_5")
                        .is_success(false)
                        .into(),
                ],
            ),
            SummaryBlock::new(
                Role::User,
                vec![SummaryMessage::content("Great! Everything looks good.")],
            ),
        ];

        let context_summary = ContextSummary { messages };
        let data = serde_json::json!({"messages": context_summary.messages});

        let actual = render_template(&data);

        insta::assert_snapshot!(actual);
    }

    #[tokio::test]
    async fn test_render_summary_frame_snapshot() {
        // Load the conversation fixture
        let fixture_json = forge_test_kit::fixture!("/src/fixtures/conversation.json").await;

        let conversation: forge_domain::Conversation =
            serde_json::from_str(&fixture_json).expect("Failed to parse conversation fixture");

        // Extract context from conversation
        let context = conversation
            .context
            .expect("Conversation should have context");

        // Create compactor instance for transformer access
        let environment = test_environment().cwd(PathBuf::from(
            "/Users/tushar/Documents/Projects/code-forge-workspace/code-forge",
        ));
        let compactor = Compactor::new(Compact::new(), environment);

        // Create context summary with tool call information
        let context_summary = ContextSummary::from(&context);

        // Apply transformers to reduce redundant operations and clean up
        let context_summary = compactor.transform(context_summary);

        let data = serde_json::json!({"messages": context_summary.messages});

        let summary = render_template(&data);

        insta::assert_snapshot!(summary);

        // Perform a full compaction
        let compacted_context = compactor.compact(context, true).unwrap();

        insta::assert_yaml_snapshot!(compacted_context);
    }

    #[test]
    fn test_compaction_removes_droppable_messages() {
        use forge_domain::{ContextMessage, Role, TextMessage};

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        // Create a context with droppable attachment messages
        let context = Context::default()
            .add_message(ContextMessage::user("User message 1", None))
            .add_message(ContextMessage::assistant(
                "Assistant response 1",
                None,
                None,
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::User, "Attachment content").droppable(true),
            ))
            .add_message(ContextMessage::user("User message 2", None))
            .add_message(ContextMessage::assistant(
                "Assistant response 2",
                None,
                None,
            ));

        let actual = compactor.compress_single_sequence(context, (0, 1)).unwrap();

        // The compaction should remove the droppable message
        // Expected: [U-summary, U2, A2]
        assert_eq!(actual.messages.len(), 3);

        // Verify the droppable attachment message was removed
        for msg in &actual.messages {
            if let ContextMessage::Text(text_msg) = msg {
                assert!(!text_msg.droppable, "Droppable messages should be removed");
            }
        }
    }

    #[test]
    fn test_compaction_clears_usage_for_token_recalculation() {
        use forge_domain::{TokenCount, Usage};

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        // Create a context with messages and usage data
        let original_usage = Usage {
            total_tokens: TokenCount::Actual(50000),
            prompt_tokens: TokenCount::Actual(45000),
            completion_tokens: TokenCount::Actual(5000),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(1.5),
        };

        let msg1 = ContextMessage::user("Message 1", None);
        let msg2 = ContextMessage::assistant("Response 1", None, None);
        let msg3 = ContextMessage::user("Message 2", None);
        let msg4 = ContextMessage::assistant("Response 2", None, None);
        let msg5 = ContextMessage::user("Message 3", None);
        let msg6 = ContextMessage::assistant("Response 3", None, None);

        let context = Context::default()
            .add_message(msg1.clone())
            .add_message(msg2.clone())
            .add_message(msg3.clone())
            .add_message(msg4.clone())
            .add_message(msg5.clone())
            .add_message(msg6.clone())
            .usage(original_usage.clone());

        // Verify usage exists before compaction
        assert_eq!(context.usage, Some(original_usage.clone()));
        assert_eq!(context.token_count(), TokenCount::Actual(50000));

        // Calculate expected token count after compaction
        // After compacting indices 0-3, we'll have:
        // 1. A summary message (replacing indices 0-3)
        // 2. Message 5 (index 4 -> "Message 3")
        // 3. Message 6 (index 5 -> "Response 3")
        let expected_tokens_after_compaction =
            msg5.token_count_approx() + msg6.token_count_approx();

        // Compact the sequence (first 4 messages, indices 0-3)
        let compacted = compactor.compress_single_sequence(context, (0, 3)).unwrap();

        // Verify we have exactly 3 messages after compaction
        assert_eq!(
            compacted.messages.len(),
            3,
            "Expected 3 messages after compaction: summary + 2 remaining messages"
        );

        // Verify usage is cleared after compaction
        assert_eq!(
            compacted.usage, None,
            "Usage field should be None after compaction to force token recalculation"
        );

        // Verify token_count returns approximation based on actual messages
        let token_count = compacted.token_count();
        assert!(
            matches!(token_count, TokenCount::Approx(_)),
            "Expected TokenCount::Approx after compaction, but got {:?}",
            token_count
        );

        // Verify the exact token count matches expected calculation
        // Note: Summary message tokens + remaining message tokens
        let actual_tokens = *token_count;
        let summary_tokens = compacted.messages[0].token_count_approx();

        assert_eq!(
            actual_tokens,
            summary_tokens + expected_tokens_after_compaction,
            "Token count should equal summary tokens ({}) + remaining message tokens ({})",
            summary_tokens,
            expected_tokens_after_compaction
        );

        // Verify it's significantly less than original
        assert!(
            actual_tokens < 50000,
            "Compacted token count ({}) should be less than original (50000)",
            actual_tokens
        );
    }
}
