use std::sync::Arc;

use forge_domain::{
    Compact, CompactionStrategy, Context, ContextMessage, ContextSummary, DropRole,
    KeepFirstUserMessage, Role, Template, Transformer, TrimContextSummary,
};
use tracing::info;

use crate::agent::AgentService;

/// A service dedicated to handling context compaction.
pub struct Compactor<S> {
    services: Arc<S>,
    compact: Compact,
}

impl<S: AgentService> Compactor<S> {
    pub fn new(services: Arc<S>, compact: Compact) -> Self {
        Self { services, compact }
    }

    /// Apply compaction to the context if requested.
    pub async fn compact(&self, context: Context, max: bool) -> anyhow::Result<Context> {
        let eviction = CompactionStrategy::evict(self.compact.eviction_window);
        let retention = CompactionStrategy::retain(self.compact.retention_window);

        let strategy = if max {
            // TODO: Consider using `eviction.max(retention)`
            retention
        } else {
            eviction.min(retention)
        };

        match strategy.eviction_range(&context) {
            Some(sequence) => self.compress_single_sequence(context, sequence).await,
            None => Ok(context),
        }
    }

    /// Compress a single identified sequence of assistant messages.
    async fn compress_single_sequence(
        &self,
        mut context: Context,
        sequence: (usize, usize),
    ) -> anyhow::Result<Context> {
        let (start, end) = sequence;

        // The sequence from the original message that needs to be compacted
        let compaction_sequence = &context.messages[start..=end].to_vec();

        // Create a temporary context for the sequence to generate summary
        let sequence_context = Context::default().messages(compaction_sequence.clone());

        // Generate context summary with tool call information
        let mut context_summary = ContextSummary::from(&sequence_context);

        // Apply transformers to reduce redundant operations and clean up
        context_summary = DropRole::new(Role::System)
            .pipe(TrimContextSummary)
            .pipe(KeepFirstUserMessage)
            .transform(context_summary);

        info!(
            sequence_start = sequence.0,
            sequence_end = sequence.1,
            sequence_length = compaction_sequence.len(),
            "Created context compaction summary"
        );

        let summary = self
            .services
            .render(
                Template::new("{{> forge-partial-summary-frame.md}}"),
                &serde_json::json!({
                    "messages": context_summary.messages
                }),
            )
            .await?;

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
        Ok(context)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    struct MockService;

    #[async_trait::async_trait]
    impl AgentService for MockService {
        async fn chat_agent(
            &self,
            _: &forge_domain::ModelId,
            _: Context,
            _: Option<forge_domain::ProviderId>,
        ) -> forge_domain::ResultStream<forge_domain::ChatCompletionMessage, anyhow::Error>
        {
            unimplemented!()
        }

        async fn call(
            &self,
            _: &forge_domain::Agent,
            _: &forge_domain::ToolCallContext,
            _: forge_domain::ToolCallFull,
        ) -> forge_domain::ToolResult {
            unimplemented!()
        }

        async fn render<V: serde::Serialize + Send + Sync>(
            &self,
            _: forge_domain::Template<V>,
            _: &V,
        ) -> anyhow::Result<String> {
            Ok("Summary frame".to_string())
        }

        async fn update(&self, _: forge_domain::Conversation) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_compress_single_sequence_preserves_only_last_reasoning() {
        use forge_domain::ReasoningFull;

        let compactor = Compactor::new(Arc::new(MockService), Compact::new());

        let first_reasoning = vec![ReasoningFull {
            text: Some("First thought".to_string()),
            signature: Some("sig1".to_string()),
        }];

        let last_reasoning = vec![ReasoningFull {
            text: Some("Last thought".to_string()),
            signature: Some("sig2".to_string()),
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

        let actual = compactor
            .compress_single_sequence(context, (0, 3))
            .await
            .unwrap();

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

    #[tokio::test]
    async fn test_compress_single_sequence_no_reasoning_accumulation() {
        use forge_domain::ReasoningFull;

        let compactor = Compactor::new(Arc::new(MockService), Compact::new());

        let reasoning = vec![ReasoningFull {
            text: Some("Original thought".to_string()),
            signature: Some("sig1".to_string()),
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

        let context = compactor
            .compress_single_sequence(context, (0, 1))
            .await
            .unwrap();

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

        let context = compactor
            .compress_single_sequence(context, (0, 2))
            .await
            .unwrap();

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

    #[tokio::test]
    async fn test_compress_single_sequence_filters_empty_reasoning() {
        use forge_domain::ReasoningFull;

        let compactor = Compactor::new(Arc::new(MockService), Compact::new());

        let non_empty_reasoning = vec![ReasoningFull {
            text: Some("Valid thought".to_string()),
            signature: Some("sig1".to_string()),
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

        let actual = compactor
            .compress_single_sequence(context, (0, 3))
            .await
            .unwrap();

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

    fn user_msg(content: &str) -> forge_domain::SummaryMessage {
        forge_domain::SummaryMessage {
            role: forge_domain::Role::User,
            messages: vec![forge_domain::SummaryMessageBlock {
                content: Some(content.to_string()),
                tool_call_id: None,
                tool_call: None,
                tool_call_success: None,
            }],
        }
    }

    fn assistant_msg(
        blocks: Vec<forge_domain::SummaryMessageBlock>,
    ) -> forge_domain::SummaryMessage {
        forge_domain::SummaryMessage { role: forge_domain::Role::Assistant, messages: blocks }
    }

    async fn render_template(data: &serde_json::Value) -> String {
        let handlebars = crate::create_handlebars();
        handlebars
            .render("forge-partial-summary-frame.md", data)
            .unwrap()
    }

    #[tokio::test]
    async fn test_render_summary_frame_snapshot() {
        use forge_domain::SummaryMessageBlock;

        let messages = vec![
            user_msg("Analyze authentication system"),
            assistant_msg(vec![
                SummaryMessageBlock::read("/src/auth.rs"),
                SummaryMessageBlock::read("/src/config.rs"),
            ]),
            user_msg("Refactor the authentication logic"),
            assistant_msg(vec![
                SummaryMessageBlock::update("/src/auth.rs")
                    .content("Refactored authentication logic"),
                SummaryMessageBlock::update("/src/config.rs").content("Updated configuration"),
            ]),
            user_msg("Add tests for authentication"),
            assistant_msg(vec![
                SummaryMessageBlock::read("/tests/auth_test.rs").tool_call_success(false),
                SummaryMessageBlock::update("/tests/auth_test.rs").content("Created new test file"),
            ]),
            user_msg("Remove deprecated authentication files"),
            assistant_msg(vec![
                SummaryMessageBlock::remove("/src/old_auth.rs"),
                SummaryMessageBlock::remove("/src/deprecated.rs"),
            ]),
        ];

        let data = serde_json::json!({
            "messages": messages
        });

        let actual = render_template(&data).await;

        insta::assert_snapshot!(actual);
    }
}
