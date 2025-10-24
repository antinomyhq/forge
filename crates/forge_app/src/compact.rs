use std::sync::Arc;

use forge_domain::{
    Agent, ChatCompletionMessage, ChatCompletionMessageFull, Compact, CompactionStrategy, Context,
    ContextMessage, ResultStreamExt, extract_tag_content,
};
use futures::Stream;
use tracing::{debug, info};

use crate::agent::AgentService;

/// A service dedicated to handling context compaction.
pub struct Compactor<S> {
    services: Arc<S>,
}

impl<S: AgentService> Compactor<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Apply compaction to the context if requested.
    pub async fn compact(
        &self,
        agent: &Agent,
        context: Context,
        max: bool,
    ) -> anyhow::Result<Context> {
        if let Some(mut compact) = agent.compact.clone() {
            debug!(agent_id = %agent.id, "Context compaction triggered");

            // If compact doesn't have a model but agent does, use agent's model
            if compact.model.is_none()
                && let Some(ref agent_model) = agent.model
            {
                compact.model = Some(agent_model.clone());
            }

            let eviction = CompactionStrategy::evict(compact.eviction_window);
            let retention = CompactionStrategy::retain(compact.retention_window);

            let strategy = if max {
                retention
            } else {
                eviction.min(retention)
            };

            match strategy.eviction_range(&context) {
                Some(sequence) => {
                    debug!(agent_id = %agent.id, "Compressing sequence");
                    self.compress_single_sequence(&compact, context, sequence)
                        .await
                }
                None => {
                    debug!(agent_id = %agent.id, "No compressible sequences found");
                    Ok(context)
                }
            }
        } else {
            Ok(context)
        }
    }

    /// Compress a single identified sequence of assistant messages.
    async fn compress_single_sequence(
        &self,
        compact: &Compact,
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
            .generate_summary_for_sequence(compact, compaction_sequence)
            .await?;

        // Accumulate the usage from the summarization call into the context
        context.usage = Some(
            context
                .usage
                .take()
                .unwrap_or_default()
                .accumulate(&summary.usage),
        );

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
        compact: &Compact,
        messages: &[ContextMessage],
    ) -> anyhow::Result<ChatCompletionMessageFull> {
        // Start with the original sequence context to preserve message structure
        let mut context = messages
            .iter()
            .fold(Context::default(), |ctx, msg| ctx.add_message(msg.clone()));

        let summary_tag = compact.summary_tag.as_ref().cloned().unwrap_or_default();
        let ctx = serde_json::json!({
            "summary_tag": summary_tag
        });

        // Render the summarization request as a user message instead of system prompt
        let prompt = self
            .services
            .render(
                compact
                    .prompt
                    .as_deref()
                    .unwrap_or("{{> forge-system-prompt-context-summarizer.md}}"),
                &ctx,
            )
            .await?;

        // Use compact.model if specified, otherwise fall back to the agent's model
        let model = compact
            .model
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No model specified for compaction"))?;

        // Add the summarization request as a user message to the existing context
        context = context.add_message(ContextMessage::user(prompt, Some(model.clone())));

        if let Some(max_token) = compact.max_tokens {
            context = context.max_tokens(max_token);
        }

        let response = self.services.chat_agent(model, context).await?;

        self.collect_completion_stream_content(compact, response)
            .await
    }

    /// Collects the content from a streaming ChatCompletionMessage response and
    /// extracts summary content from the specified tag
    async fn collect_completion_stream_content(
        &self,
        compact: &Compact,
        stream: impl Stream<Item = anyhow::Result<ChatCompletionMessage>>
        + std::marker::Unpin
        + ResultStreamExt<anyhow::Error>,
    ) -> anyhow::Result<ChatCompletionMessageFull> {
        let mut response = stream.into_full(false).await?;

        // Extract content from summary tag if present
        if let Some(extracted) = extract_tag_content(
            &response.content,
            compact
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

#[cfg(test)]
mod tests {
    use forge_domain::{ChatCompletionMessage, Content, FinishReason, ModelId, TokenCount, Usage};
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
        let compactor = Compactor::new(Arc::new(MockService::with_usage(0.005)));
        let context = Context::default()
            .add_message(ContextMessage::user("M1", None))
            .add_message(ContextMessage::assistant("R1", None, None))
            .usage(usage(0.010));

        let actual = compactor
            .compress_single_sequence(&Compact::new().model(ModelId::new("m")), context, (0, 1))
            .await
            .unwrap();

        assert_eq!(actual.usage.unwrap().cost, Some(0.015));
    }

    #[tokio::test]
    async fn test_compress_single_sequence_preserves_only_last_reasoning() {
        use forge_domain::ReasoningFull;

        let compactor = Compactor::new(Arc::new(MockService::with_usage(0.005)));

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
            .compress_single_sequence(&Compact::new().model(ModelId::new("m")), context, (0, 3))
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

        let compactor = Compactor::new(Arc::new(MockService::with_usage(0.005)));

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
            .compress_single_sequence(&Compact::new().model(ModelId::new("m")), context, (0, 1))
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
            .compress_single_sequence(&Compact::new().model(ModelId::new("m")), context, (0, 2))
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

        let compactor = Compactor::new(Arc::new(MockService::with_usage(0.005)));

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
            .compress_single_sequence(&Compact::new().model(ModelId::new("m")), context, (0, 3))
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
}
