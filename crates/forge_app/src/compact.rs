use std::sync::Arc;

use forge_domain::{
    ChatCompletionMessage, ChatCompletionMessageFull, Compact, CompactionStrategy, Context,
    ContextMessage, ResultStreamExt, TokenCount, Usage, extract_tag_content,
};
use futures::Stream;
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

        // Extract user messages from the sequence to pass as feedback
        let feedback: Vec<String> = compaction_sequence
            .iter()
            .filter(|msg| msg.has_role(forge_domain::Role::User))
            .filter_map(|msg| msg.content().map(|content| content.to_string()))
            .collect();

        // Generate summary for the compaction sequence
        let summary = self
            .generate_summary_for_sequence(compaction_sequence)
            .await?;

        // Accumulate the usage from the summarization call into the context
        context.usage = create_usage(context.usage, Some(summary.usage));

        let summary = summary.content;

        info!(
            summary = %summary,
            sequence_start = sequence.0,
            sequence_end = sequence.1,
            sequence_length = compaction_sequence.len(),
            "Created context compaction summary"
        );

        let summary = self
            .services
            .render(
                "{{> forge-partial-summary-frame.md}}",
                &serde_json::json!({"summary": summary, "feedback": feedback}),
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

    /// Generate a summary for a specific sequence of assistant messages.
    /// Returns ChatCompletionMessageFull with extracted summary content
    async fn generate_summary_for_sequence(
        &self,
        messages: &[ContextMessage],
    ) -> anyhow::Result<ChatCompletionMessageFull> {
        // Start with the original sequence context to preserve message structure
        let mut context = messages
            .iter()
            .fold(Context::default(), |ctx, msg| ctx.add_message(msg.clone()));

        let summary_tag = self
            .compact
            .summary_tag
            .as_ref()
            .cloned()
            .unwrap_or_default();
        let ctx = serde_json::json!({
            "summary_tag": summary_tag
        });

        // Render the summarization request as a user message instead of system prompt
        let prompt = self
            .services
            .render(
                self.compact
                    .prompt
                    .as_deref()
                    .unwrap_or("{{> forge-system-prompt-context-summarizer.md}}"),
                &ctx,
            )
            .await?;

        // Use compact.model if specified, otherwise fall back to the agent's model
        let model = self
            .compact
            .model
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No model specified for compaction"))?;

        // Add the summarization request as a user message to the existing context
        context = context.add_message(ContextMessage::user(prompt, Some(model.clone())));

        if let Some(max_token) = self.compact.max_tokens {
            context = context.max_tokens(max_token);
        }

        let response = self.services.chat_agent(model, context, None).await?;

        self.collect_completion_stream_content(response).await
    }

    /// Collects the content from a streaming ChatCompletionMessage response and
    /// extracts summary content from the specified tag
    async fn collect_completion_stream_content(
        &self,
        stream: impl Stream<Item = anyhow::Result<ChatCompletionMessage>>
        + std::marker::Unpin
        + ResultStreamExt<anyhow::Error>,
    ) -> anyhow::Result<ChatCompletionMessageFull> {
        let mut response = stream.into_full(false).await?;

        // Extract content from summary tag if present
        if let Some(extracted) = extract_tag_content(
            &response.content,
            self.compact
                .summary_tag
                .as_ref()
                .cloned()
                .unwrap_or_default()
                .as_str(),
        ) {
            response.content = extracted.to_string();
        }

        Ok(response)
    }
}

fn create_usage(before: Option<Usage>, summary: Option<Usage>) -> Option<Usage> {
    let (Some(before), Some(summary)) = (before, summary) else {
        return None;
    };

    // After
    let prompt_tokens = TokenCount::Approx(
        *summary.completion_tokens + *before.prompt_tokens - *summary.prompt_tokens,
    );
    let completion_tokens = before.completion_tokens;
    Some(Usage {
        total_tokens: prompt_tokens + completion_tokens,
        cached_tokens: TokenCount::default(),
        cost: zip_with(before.cost, summary.cost, |a, b| a + b),
        prompt_tokens,
        completion_tokens,
    })
}

fn zip_with<A, F: FnOnce(A, A) -> A>(a: Option<A>, b: Option<A>, f: F) -> Option<A> {
    match (a, b) {
        (None, None) => None,
        (None, b) => b,
        (a, None) => a,
        (Some(a), Some(b)) => Some(f(a, b)),
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        ChatCompletionMessage, Content, FinishReason, ModelId, ProviderId, TokenCount, Usage,
    };
    use pretty_assertions::assert_eq;

    use super::*;

    struct MockService {
        response: ChatCompletionMessageFull,
    }

    impl MockService {
        fn with_usage(cost: f64) -> Self {
            Self {
                response: ChatCompletionMessageFull {
                    content: "Summary".to_string(),
                    usage: Usage {
                        prompt_tokens: TokenCount::Actual(100),
                        completion_tokens: TokenCount::Actual(50),
                        total_tokens: TokenCount::Actual(150),
                        cached_tokens: TokenCount::Actual(0),
                        cost: Some(cost),
                    },
                    tool_calls: vec![],
                    reasoning: None,
                    reasoning_details: None,
                    finish_reason: Some(FinishReason::Stop),
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl AgentService for MockService {
        async fn chat_agent(
            &self,
            _: &ModelId,
            _: Context,
            _: Option<ProviderId>,
        ) -> forge_domain::ResultStream<ChatCompletionMessage, anyhow::Error> {
            let msg = ChatCompletionMessage::default()
                .content(Content::full(self.response.content.clone()))
                .usage(self.response.usage.clone())
                .finish_reason(FinishReason::Stop);
            Ok(Box::pin(tokio_stream::iter(std::iter::once(Ok(msg)))))
        }

        async fn call(
            &self,
            _: &forge_domain::Agent,
            _: &forge_domain::ToolCallContext,
            _: forge_domain::ToolCallFull,
        ) -> forge_domain::ToolResult {
            unimplemented!()
        }

        async fn render(
            &self,
            _: &str,
            _: &(impl serde::Serialize + Sync),
        ) -> anyhow::Result<String> {
            Ok("Summary frame".to_string())
        }

        async fn update(&self, _: forge_domain::Conversation) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn usage(cost: f64) -> Usage {
        Usage {
            prompt_tokens: TokenCount::Actual(200),
            completion_tokens: TokenCount::Actual(100),
            total_tokens: TokenCount::Actual(300),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(cost),
        }
    }

    #[tokio::test]
    async fn test_compress_single_sequence_accumulates_usage() {
        let compactor = Compactor::new(
            Arc::new(MockService::with_usage(0.005)),
            Compact::new().model(ModelId::new("m")),
        );
        let context = Context::default()
            .add_message(ContextMessage::user("M1", None))
            .add_message(ContextMessage::assistant("R1", None, None))
            .usage(usage(0.010));

        let actual = compactor
            .compress_single_sequence(context, (0, 1))
            .await
            .unwrap();

        assert_eq!(actual.usage.unwrap().cost, Some(0.015));
    }

    #[tokio::test]
    async fn test_compress_single_sequence_preserves_only_last_reasoning() {
        use forge_domain::ReasoningFull;

        let compactor = Compactor::new(
            Arc::new(MockService::with_usage(0.005)),
            Compact::new().model(ModelId::new("m")),
        );

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

        let compactor = Compactor::new(
            Arc::new(MockService::with_usage(0.005)),
            Compact::new().model(ModelId::new("m")),
        );

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

        let compactor = Compactor::new(
            Arc::new(MockService::with_usage(0.005)),
            Compact::new().model(ModelId::new("m")),
        );

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

    #[test]
    fn test_create_usage_none_inputs() {
        // Test when both inputs are None
        let actual = create_usage(None, None);
        assert_eq!(actual, None);

        // Test when only before is None
        let summary = usage(0.005);
        let actual = create_usage(None, Some(summary));
        assert_eq!(actual, None);

        // Test when only summary is None
        let before = usage(0.010);
        let actual = create_usage(Some(before), None);
        assert_eq!(actual, None);
    }

    #[test]
    fn test_create_usage_basic_calculation() {
        let before = Usage {
            prompt_tokens: TokenCount::Actual(200),
            completion_tokens: TokenCount::Actual(100),
            total_tokens: TokenCount::Actual(300),
            cached_tokens: TokenCount::Actual(10),
            cost: Some(0.010),
        };

        let summary = Usage {
            prompt_tokens: TokenCount::Actual(50),
            completion_tokens: TokenCount::Actual(25),
            total_tokens: TokenCount::Actual(75),
            cached_tokens: TokenCount::Actual(5),
            cost: Some(0.005),
        };

        let actual = create_usage(Some(before), Some(summary)).unwrap();

        let expected = Usage {
            prompt_tokens: TokenCount::Approx(175), // 25 + 200 - 50 = 175
            completion_tokens: TokenCount::Actual(100), // Preserved from before
            total_tokens: TokenCount::Approx(275),  // 175 + 100 = 275
            cached_tokens: TokenCount::default(),   // Reset to 0
            cost: Some(0.015),                      // 0.010 + 0.005 = 0.015
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_usage_with_actual_token_counts() {
        let before = Usage {
            prompt_tokens: TokenCount::Actual(1000),
            completion_tokens: TokenCount::Actual(500),
            total_tokens: TokenCount::Actual(1500),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(0.020),
        };

        let summary = Usage {
            prompt_tokens: TokenCount::Actual(300),
            completion_tokens: TokenCount::Actual(150),
            total_tokens: TokenCount::Actual(450),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(0.008),
        };

        let actual = create_usage(Some(before), Some(summary)).unwrap();

        let expected = Usage {
            prompt_tokens: TokenCount::Approx(850), // 150 + 1000 - 300 = 850
            completion_tokens: TokenCount::Actual(500), // Preserved from before
            total_tokens: TokenCount::Approx(1350), // 850 + 500 = 1350
            cached_tokens: TokenCount::default(),   // Reset to 0
            cost: Some(0.028),                      // 0.020 + 0.008 = 0.028
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_usage_with_approx_token_counts() {
        let before = Usage {
            prompt_tokens: TokenCount::Approx(800),
            completion_tokens: TokenCount::Approx(400),
            total_tokens: TokenCount::Approx(1200),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(0.015),
        };

        let summary = Usage {
            prompt_tokens: TokenCount::Approx(200),
            completion_tokens: TokenCount::Approx(100),
            total_tokens: TokenCount::Approx(300),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(0.007),
        };

        let actual = create_usage(Some(before), Some(summary)).unwrap();

        let expected = Usage {
            prompt_tokens: TokenCount::Approx(700), // 100 + 800 - 200 = 700
            completion_tokens: TokenCount::Approx(400), // Preserved from before
            total_tokens: TokenCount::Approx(1100), // 700 + 400 = 1100
            cached_tokens: TokenCount::default(),   // Reset to 0
            cost: Some(0.022),                      // 0.015 + 0.007 = 0.022
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_usage_with_mixed_token_counts() {
        let before = Usage {
            prompt_tokens: TokenCount::Actual(600),
            completion_tokens: TokenCount::Approx(300),
            total_tokens: TokenCount::Actual(900),
            cached_tokens: TokenCount::Actual(20),
            cost: Some(0.012),
        };

        let summary = Usage {
            prompt_tokens: TokenCount::Approx(150),
            completion_tokens: TokenCount::Actual(75),
            total_tokens: TokenCount::Approx(225),
            cached_tokens: TokenCount::Actual(10),
            cost: Some(0.006),
        };

        let actual = create_usage(Some(before), Some(summary)).unwrap();

        let expected = Usage {
            prompt_tokens: TokenCount::Approx(525), // 75 + 600 - 150 = 525
            completion_tokens: TokenCount::Approx(300), // Preserved from before
            total_tokens: TokenCount::Approx(825),  // 525 + 300 = 825
            cached_tokens: TokenCount::default(),   // Reset to 0
            cost: Some(0.018000000000000002),       // Float precision: 0.012 + 0.006
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_usage_without_costs() {
        let before = Usage {
            prompt_tokens: TokenCount::Actual(400),
            completion_tokens: TokenCount::Actual(200),
            total_tokens: TokenCount::Actual(600),
            cached_tokens: TokenCount::Actual(0),
            cost: None,
        };

        let summary = Usage {
            prompt_tokens: TokenCount::Actual(100),
            completion_tokens: TokenCount::Actual(50),
            total_tokens: TokenCount::Actual(150),
            cached_tokens: TokenCount::Actual(0),
            cost: None,
        };

        let actual = create_usage(Some(before), Some(summary)).unwrap();

        let expected = Usage {
            prompt_tokens: TokenCount::Approx(350), // 50 + 400 - 100 = 350
            completion_tokens: TokenCount::Actual(200), // Preserved from before
            total_tokens: TokenCount::Approx(550),  // 350 + 200 = 550
            cached_tokens: TokenCount::default(),   // Reset to 0
            cost: None,                             // No costs in either input
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_usage_with_partial_costs() {
        let before = Usage {
            prompt_tokens: TokenCount::Actual(300),
            completion_tokens: TokenCount::Actual(150),
            total_tokens: TokenCount::Actual(450),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(0.008),
        };

        let summary = Usage {
            prompt_tokens: TokenCount::Actual(80),
            completion_tokens: TokenCount::Actual(40),
            total_tokens: TokenCount::Actual(120),
            cached_tokens: TokenCount::Actual(0),
            cost: None,
        };

        let actual = create_usage(Some(before), Some(summary)).unwrap();

        let expected = Usage {
            prompt_tokens: TokenCount::Approx(260), // 40 + 300 - 80 = 260
            completion_tokens: TokenCount::Actual(150), // Preserved from before
            total_tokens: TokenCount::Approx(410),  // 260 + 150 = 410
            cached_tokens: TokenCount::default(),   // Reset to 0
            cost: Some(0.008),                      // Only before cost preserved
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_usage_zero_values() {
        let before = Usage {
            prompt_tokens: TokenCount::Actual(0),
            completion_tokens: TokenCount::Actual(0),
            total_tokens: TokenCount::Actual(0),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(0.0),
        };

        let summary = Usage {
            prompt_tokens: TokenCount::Actual(0),
            completion_tokens: TokenCount::Actual(0),
            total_tokens: TokenCount::Actual(0),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(0.0),
        };

        let actual = create_usage(Some(before), Some(summary)).unwrap();

        let expected = Usage {
            prompt_tokens: TokenCount::Approx(0),     // 0 + 0 - 0 = 0
            completion_tokens: TokenCount::Actual(0), // Preserved from before
            total_tokens: TokenCount::Approx(0),      // 0 + 0 = 0
            cached_tokens: TokenCount::default(),     // Reset to 0
            cost: Some(0.0),                          // 0.0 + 0.0 = 0.0
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_usage_large_numbers() {
        let before = Usage {
            prompt_tokens: TokenCount::Actual(1000000),
            completion_tokens: TokenCount::Actual(500000),
            total_tokens: TokenCount::Actual(1500000),
            cached_tokens: TokenCount::Actual(100000),
            cost: Some(10.50),
        };

        let summary = Usage {
            prompt_tokens: TokenCount::Actual(200000),
            completion_tokens: TokenCount::Actual(100000),
            total_tokens: TokenCount::Actual(300000),
            cached_tokens: TokenCount::Actual(20000),
            cost: Some(2.25),
        };

        let actual = create_usage(Some(before), Some(summary)).unwrap();

        let expected = Usage {
            prompt_tokens: TokenCount::Approx(900000), // 100000 + 1000000 - 200000 = 900000
            completion_tokens: TokenCount::Actual(500000), // Preserved from before
            total_tokens: TokenCount::Approx(1400000), // 900000 + 500000 = 1400000
            cached_tokens: TokenCount::default(),      // Reset to 0
            cost: Some(12.75),                         // 10.50 + 2.25 = 12.75
        };

        assert_eq!(actual, expected);
    }
}
