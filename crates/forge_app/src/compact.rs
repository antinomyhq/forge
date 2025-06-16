use std::sync::Arc;

use forge_domain::{
    Agent, ChatCompletionMessage, ChatCompletionMessageFull, Compact, Context, ContextMessage,
    ResultStreamExt, extract_tag_content, find_compact_sequence,
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
    pub async fn compact_context(
        &self,
        agent: &Agent,
        context: Context,
    ) -> anyhow::Result<Context> {
        if let Some(ref compact) = agent.compact {
            debug!(agent_id = %agent.id, "Context compaction triggered");

            match find_compact_sequence(&context, compact.retention_window)
                .into_iter()
                .next()
            {
                Some(sequence) => {
                    debug!(agent_id = %agent.id, "Compressing sequence");
                    self.compress_single_sequence(compact, context, sequence)
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

        let summary = self
            .generate_summary_for_sequence(compact, sequence_messages)
            .await?;

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
                "{{> forge-partial-summary-frame.hbs}}",
                &serde_json::json!({ "summary": summary }),
            )
            .await?;

        context.messages.splice(
            start..=end,
            std::iter::once(ContextMessage::user(summary, None)),
        );

        Ok(context)
    }

    /// Generate a summary for a specific sequence of assistant messages.
    async fn generate_summary_for_sequence(
        &self,
        compact: &Compact,
        messages: &[ContextMessage],
    ) -> anyhow::Result<String> {
        let sequence_context = messages
            .iter()
            .fold(Context::default(), |ctx, msg| ctx.add_message(msg.clone()));

        let summary_tag = compact.summary_tag.as_ref().cloned().unwrap_or_default();
        let ctx = serde_json::json!({
            "context": sequence_context.to_text(),
            "summary_tag": summary_tag
        });

        let prompt = self
            .services
            .render(
                compact
                    .prompt
                    .as_deref()
                    .unwrap_or("{{> forge-system-prompt-context-summarizer.hbs}}"),
                &ctx,
            )
            .await?;

        let mut context = Context::default()
            .add_message(ContextMessage::user(prompt, compact.model.clone().into()));

        if let Some(max_token) = compact.max_tokens {
            context = context.max_tokens(max_token);
        }

        let response = self.services.chat(&compact.model, context).await?;

        self.collect_completion_stream_content(compact, response)
            .await
    }

    /// Collects the content from a streaming ChatCompletionMessage response.
    async fn collect_completion_stream_content(
        &self,
        compact: &Compact,
        stream: impl Stream<Item = anyhow::Result<ChatCompletionMessage>>
        + std::marker::Unpin
        + ResultStreamExt<anyhow::Error>,
    ) -> anyhow::Result<String> {
        let ChatCompletionMessageFull { content, .. } = stream.into_full(false).await?;
        if let Some(extracted) = extract_tag_content(
            &content,
            compact
                .summary_tag
                .as_ref()
                .cloned()
                .unwrap_or_default()
                .as_str(),
        ) {
            return Ok(extracted.to_string());
        }

        Ok(content)
    }
}
