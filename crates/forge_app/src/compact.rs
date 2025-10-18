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

        let sequence_messages = &context.messages[start..=end].to_vec();

        // Extract user messages from the sequence to pass as feedback
        let feedback: Vec<String> = sequence_messages
            .iter()
            .filter(|msg| msg.has_role(forge_domain::Role::User))
            .filter_map(|msg| msg.content().map(|content| content.to_string()))
            .collect();

        // Generate summary for the compaction sequence
        let summary = self
            .generate_summary_for_sequence(compact, sequence_messages)
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
            sequence_length = sequence_messages.len(),
            "Created context compaction summary"
        );

        let summary = self
            .services
            .render(
                "{{> forge-partial-summary-frame.md}}",
                &serde_json::json!({"summary": summary, "feedback": feedback}),
            )
            .await?;

        context.messages.splice(
            start..=end,
            std::iter::once(ContextMessage::user(summary, None)),
        );

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
}
