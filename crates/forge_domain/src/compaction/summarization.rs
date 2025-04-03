use std::cmp::min;
/// Summarization strategy for context compaction
///
/// This strategy identifies sequences of assistant messages in a conversation
/// and replaces them with a single summarized message, preserving user messages
/// and maintaining conversation continuity while reducing token usage.
use std::sync::Arc;

use anyhow::Result;
use tracing::debug;

use super::adjust_range::adjust_range_for_tool_calls;
use super::strategy::CompactionImpact;
use super::CompactionStrategy;
use crate::services::{Services, TemplateService};
use crate::{Compact, Context, ContextMessage, Role};

/// Compaction strategy that identifies sequences of messages and replaces them
/// with a summary generated by an LLM
pub struct SummarizationStrategy<S> {
    services: Arc<S>,
}

impl<S: Services> SummarizationStrategy<S> {
    /// Creates a new SummarizationStrategy with the given services
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }
}

impl<S: Services> CompactionStrategy for SummarizationStrategy<S> {
    fn id(&self) -> &'static str {
        "summarization"
    }

    fn is_applicable(&self, compact: &Compact, context: &Context) -> bool {
        let preserve_last_n = compact.retention_window;
        find_sequence(context, preserve_last_n).is_some()
    }

    async fn compact(
        &self,
        compact: &Compact,
        context: Context,
    ) -> Result<(Context, CompactionImpact)> {
        let preserve_last_n = compact.retention_window;
        let original_message_count = context.messages.len();

        // Find a sequence of messages to summarize
        if let Some((start, end)) = find_sequence(&context, preserve_last_n) {
            debug!(
                strategy = self.id(),
                start, end, "Found compressible sequence of messages"
            );

            let mut new_context = context.clone();
            let summary_text = self
                .services
                .template_service()
                .render_summarization(compact, &context)
                .await?;

            // Create a new message containing the summary
            let summary_msg = ContextMessage::assistant(summary_text, None);

            // Replace the sequence of messages with the summary
            new_context.messages.splice(start..=end, [summary_msg]);

            let impact = CompactionImpact::new(
                original_message_count,
                new_context.messages.len(),
                None, // We don't have token counts available
            );

            Ok((new_context, impact))
        } else {
            // No sequence found, return the context unchanged with zero impact
            let impact =
                CompactionImpact::new(original_message_count, original_message_count, Some(0));
            Ok((context, impact))
        }
    }
}

/// Finds all valid compressible sequences in the context, respecting the
/// preservation window
///
/// This function identifies sequences of assistant messages between user
/// messages that can be compressed by summarization, while respecting the
/// preservation window.
fn find_sequence(context: &Context, preserve_last_n: usize) -> Option<(usize, usize)> {
    let messages = &context.messages;
    if messages.is_empty() {
        return None;
    }

    // len will be always > 0
    let length = messages.len();
    let mut max_len = length - min(length, preserve_last_n);

    if max_len == 0 {
        return None;
    }

    // Additional check: if max_len < 1, we can't safely do max_len - 1
    if max_len < 1 {
        return None;
    }
    if messages
        .get(max_len - 1)
        .is_some_and(|msg| msg.has_tool_call())
    {
        max_len -= 1;
    }

    let user_messages = messages
        .iter()
        .enumerate()
        .take(max_len)
        .filter(|(_, message)| message.has_role(Role::User))
        .collect::<Vec<_>>();

    // If there are no user messages, there can't be any sequences
    if user_messages.is_empty() {
        return None;
    }
    let start_positions = user_messages
        .iter()
        .map(|(start, _)| min(start.saturating_add(1), max_len.saturating_sub(1)))
        .collect::<Vec<_>>();

    let mut end_positions = user_messages
        .iter()
        .skip(1)
        .map(|(pos, _)| pos.saturating_sub(1))
        .collect::<Vec<_>>();
    end_positions.push(max_len - 1);

    // If either vector is empty, there can't be any compressible sequences
    if start_positions.is_empty() || end_positions.is_empty() {
        return None;
    }

    // Find a valid sequence and adjust it to preserve tool call chains
    let range = start_positions
        .iter()
        .zip(end_positions.iter())
        .find(|(start, end)| *end > *start)
        .map(|(a, b)| (*a, *b));

    // If we found a range, adjust it to respect tool call chains
    if let Some((start, end)) = range {
        let (adjusted_start, adjusted_end) = adjust_range_for_tool_calls(context, start, end);
        return Some((adjusted_start, adjusted_end));
    }

    None
}
