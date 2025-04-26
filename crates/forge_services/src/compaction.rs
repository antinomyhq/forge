use std::sync::Arc;

use anyhow::Result;
use forge_domain::{
    extract_tag_content, Agent, ChatCompletionMessage, Compact, CompactionService, Context,
    ContextMessage, ProviderService, Role, TemplateService,
};
use futures::StreamExt;
use tracing::{debug, info};

/// Handles the compaction of conversation contexts to manage token usage
#[derive(Clone)]
pub struct ForgeCompactionService<T, P> {
    template: Arc<T>,
    provider: Arc<P>,
}

impl<T: TemplateService, P: ProviderService> ForgeCompactionService<T, P> {
    /// Creates a new ContextCompactor instance
    pub fn new(template: Arc<T>, provider: Arc<P>) -> Self {
        Self { template, provider }
    }

    /// Apply compaction to the context if requested
    pub async fn compact_context(&self, agent: &Agent, context: Context) -> Result<Context> {
        // Return early if agent doesn't have compaction configured
        if let Some(ref compact) = agent.compact {
            debug!(agent_id = %agent.id, "Context compaction triggered");

            // Identify and compress the first compressible sequence
            // Get all compressible sequences, considering the preservation window
            match find_sequence(&context, compact.retention_window)
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

    /// Compress a single identified sequence of assistant messages
    async fn compress_single_sequence(
        &self,
        compact: &Compact,
        mut context: Context,
        sequence: (usize, usize),
    ) -> Result<Context> {
        let (start, end) = sequence;

        // Extract the sequence to summarize
        let sequence_messages = &context.messages[start..=end];

        // Generate summary for this sequence
        let summary = self
            .generate_summary_for_sequence(compact, sequence_messages)
            .await?;

        // Log the summary for debugging
        info!(
            summary = %summary,
            sequence_start = sequence.0,
            sequence_end = sequence.1,
            "Created context compaction summary"
        );

        let summary = format!(
            r#"Continuing from a prior analysis. Below is a compacted summary of the ongoing session. Use this summary as authoritative context for your reasoning and decision-making. You do not need to repeat or reanalyze it unless specifically asked: <summary>{summary}</summary> Proceed based on this context.
        "#
        );

        // Replace the sequence with a single summary message using splice
        // This removes the sequence and inserts the summary message in-place
        context.messages.splice(
            start..=end,
            std::iter::once(ContextMessage::assistant(summary, None)),
        );

        Ok(context)
    }

    /// Generate a summary for a specific sequence of assistant messages
    async fn generate_summary_for_sequence(
        &self,
        compact: &Compact,
        messages: &[ContextMessage],
    ) -> Result<String> {
        // Create a temporary context with just the sequence for summarization
        let sequence_context = messages
            .iter()
            .fold(Context::default(), |ctx, msg| ctx.add_message(msg.clone()));

        // Render the summarization prompt
        let summary_tag = compact.summary_tag.as_ref().cloned().unwrap_or_default();
        let ctx = serde_json::json!({
            "context": sequence_context.to_text(),
            "summary_tag": summary_tag
        });

        let prompt = self.template.render(
            compact
                .prompt
                .as_deref()
                .unwrap_or("{{> system-prompt-context-summarizer.hbs}}"),
            &ctx,
        )?;

        // Create a new context
        let mut context = Context::default().add_message(ContextMessage::user(prompt));

        // Set max_tokens for summary
        if let Some(max_token) = compact.max_tokens {
            context = context.max_tokens(max_token);
        }

        // Get summary from the provider
        let response = self.provider.chat(&compact.model, context).await?;

        self.collect_completion_stream_content(compact, response)
            .await
    }

    /// Collects the content from a streaming ChatCompletionMessage response
    /// and extracts text within the configured tag if present
    async fn collect_completion_stream_content<F>(
        &self,
        compact: &Compact,
        mut stream: F,
    ) -> Result<String>
    where
        F: futures::Stream<Item = Result<ChatCompletionMessage>> + Unpin,
    {
        let mut result_content = String::new();

        while let Some(message_result) = stream.next().await {
            let message = message_result?;
            if let Some(content) = message.content {
                result_content.push_str(content.as_str());
            }
        }

        // Extract content from within configured tags if present and if tag is
        // configured
        if let Some(extracted) = extract_tag_content(
            &result_content,
            compact
                .summary_tag
                .as_ref()
                .cloned()
                .unwrap_or_default()
                .as_str(),
        ) {
            return Ok(extracted.to_string());
        }

        // If no tag extraction performed, return the original content
        Ok(result_content)
    }
}

/// Finds a sequence in the context for compaction, starting from the first
/// assistant message and including all messages up to the last possible message
/// (respecting preservation window)
fn find_sequence(context: &Context, preserve_last_n: usize) -> Option<(usize, usize)> {
    let messages = &context.messages;
    if messages.is_empty() {
        return None;
    }

    // len will be always > 0
    let length = messages.len();

    // Find the first assistant message index
    let start = messages
        .iter()
        .enumerate()
        .find(|(_, message)| message.has_role(Role::Assistant))
        .map(|(index, _)| index)?;

    // Don't compact if there's no assistant message
    if start >= length {
        return None;
    }

    // Calculate the end index based on preservation window
    // If we need to preserve all or more messages than we have, there's nothing to
    // compact
    if preserve_last_n >= length {
        return None;
    }

    // Use saturating subtraction to prevent potential overflow
    let end = length.saturating_sub(preserve_last_n).saturating_sub(1);

    // Ensure we have at least two messages to create a meaningful summary
    // If start > end or end is invalid, don't compact
    if start > end || end >= length || end.saturating_sub(start) < 1 {
        return None;
    }

    // Don't break between a tool call and its result
    if messages.get(end).is_some_and(|msg| msg.has_tool_call()) {
        // If the last message has a tool call, adjust end to include the tool result
        // This means either not compacting at all, or reducing the end by 1
        if end == start {
            // If start == end and it has a tool call, don't compact
            return None;
        } else {
            // Otherwise reduce end by 1
            return Some((start, end.saturating_sub(1)));
        }
    }

    // Return the sequence only if it has at least one message
    if end >= start {
        Some((start, end))
    } else {
        None
    }
}

#[async_trait::async_trait]
impl<T: TemplateService, P: ProviderService> CompactionService for ForgeCompactionService<T, P> {
    async fn compact_context(&self, agent: &Agent, context: Context) -> anyhow::Result<Context> {
        // Call the compact_context method without passing prompt_tokens
        // since the decision logic has been moved to the orchestrator
        self.compact_context(agent, context).await
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{ToolCallFull, ToolCallId, ToolName, ToolResult};
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_identify_first_compressible_sequence() {
        // Create a context with a sequence of assistant messages
        let context = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message 1"))
            .add_message(ContextMessage::assistant("Assistant message 1", None))
            .add_message(ContextMessage::assistant("Assistant message 2", None))
            .add_message(ContextMessage::assistant("Assistant message 3", None))
            .add_message(ContextMessage::user("User message 2"))
            .add_message(ContextMessage::assistant("Assistant message 4", None));

        // With the new logic, we compact from the first assistant message (index 2)
        // through the end (respecting preservation window)
        let sequence = find_sequence(&context, 0);

        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 6); // Now includes all messages up through index 6
    }

    #[test]
    fn test_no_compressible_sequence() {
        // Create a context with only single messages - not enough for compaction
        let context = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message"))
            .add_message(ContextMessage::assistant("Assistant message", None));

        // With the updated compaction logic, we need at least two messages after
        // an assistant message to create a compressible sequence
        let sequence = find_sequence(&context, 0);
        assert!(sequence.is_none());
    }

    #[test]
    fn test_sequence_at_end_of_context() {
        // Create a context with a sequence at the end
        let context = Context::default()
            .add_message(ContextMessage::system("System message")) // 0
            .add_message(ContextMessage::user("User message 1")) // 1
            .add_message(ContextMessage::assistant("Assistant message 1", None)) // 2
            .add_message(ContextMessage::user("User message 2")) // 3
            .add_message(ContextMessage::assistant("Assistant message 2", None)) // 4
            .add_message(ContextMessage::assistant("Assistant message 3", None)); // 5

        // With the updated logic, we start from the first assistant message (index 2)
        // and include everything to the end
        let sequence = find_sequence(&context, 0);

        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2); // First assistant message
        assert_eq!(end, 5); // Last message in the context
    }

    #[test]
    fn test_identify_sequence_with_tool_calls() {
        // Create a context with assistant messages containing tool calls
        let tool_call = ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path"}),
        };

        let context = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message 1"))
            .add_message(ContextMessage::assistant(
                "Assistant message with tool call",
                Some(vec![tool_call.clone()]),
            ))
            .add_message(ContextMessage::assistant(
                "Assistant message with another tool call",
                Some(vec![tool_call.clone()]),
            ))
            .add_message(ContextMessage::user("User message 2"));

        // With the updated logic, the sequence is from index 2 to index 4 (all messages
        // from first assistant)
        let sequence = find_sequence(&context, 0);

        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 4); // Now includes the user message at the end
    }

    #[test]
    fn test_identify_sequence_with_tool_results() {
        // Create a context with assistant messages and tool results
        let tool_call = ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path"}),
        };

        let tool_result = ToolResult::new(ToolName::new("forge_tool_fs_read"))
            .call_id(ToolCallId::new("call_123"))
            .success(json!({"content": "File content"}).to_string());

        let context = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message 1"))
            .add_message(ContextMessage::assistant(
                "Assistant message with tool call",
                Some(vec![tool_call]),
            ))
            .add_message(ContextMessage::tool_result(tool_result))
            .add_message(ContextMessage::assistant(
                "Assistant follow-up message",
                None,
            ))
            .add_message(ContextMessage::assistant("Another assistant message", None))
            .add_message(ContextMessage::user("User message 2"));

        // With the updated logic, we include all messages from the first assistant
        // (index 2) through to the end (index 6)
        let sequence = find_sequence(&context, 0);

        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 6); // Now includes the user message at the end
    }

    #[test]
    fn test_mixed_assistant_and_tool_messages() {
        // Create a context with mixed assistant and tool messages
        let tool_call1 = ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path1"}),
        };

        let tool_call2 = ToolCallFull {
            name: ToolName::new("forge_tool_fs_search"),
            call_id: Some(ToolCallId::new("call_456")),
            arguments: json!({"path": "/test/path2", "regex": "pattern"}),
        };

        let tool_result1 = ToolResult::new(ToolName::new("forge_tool_fs_read"))
            .call_id(ToolCallId::new("call_123"))
            .success(json!({"content": "File content 1"}).to_string());

        let tool_result2 = ToolResult::new(ToolName::new("forge_tool_fs_search"))
            .call_id(ToolCallId::new("call_456"))
            .success(json!({"matches": ["match1", "match2"]}).to_string());

        // Create a context where we have a mix of assistant and tool messages
        let context = Context::default()
            .add_message(ContextMessage::user("User message 1")) // 0
            .add_message(ContextMessage::assistant(
                "Assistant message with tool call",
                Some(vec![tool_call1]),
            )) // 1
            .add_message(ContextMessage::tool_result(tool_result1)) // 2
            .add_message(ContextMessage::user("User follow-up question")) // 3
            .add_message(ContextMessage::assistant(
                "Assistant with another tool call",
                Some(vec![tool_call2]),
            )) // 4
            .add_message(ContextMessage::tool_result(tool_result2)) // 5
            .add_message(ContextMessage::user("User message 2")); // 6

        // With the updated compaction logic, we include all messages starting from
        // the first assistant message through the end of the context
        let sequence = find_sequence(&context, 0);

        let (start, end) = sequence.unwrap();
        assert_eq!(start, 1); // First assistant message
        assert_eq!(end, 6); // Last message in context
    }

    #[test]
    fn test_consecutive_assistant_messages_with_tools() {
        // Test when we have consecutive assistant messages with tool calls
        // followed by tool results but the assistant messages themselves are
        // consecutive
        let tool_call1 = ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path1"}),
        };

        let tool_call2 = ToolCallFull {
            name: ToolName::new("forge_tool_fs_search"),
            call_id: Some(ToolCallId::new("call_456")),
            arguments: json!({"path": "/test/path2", "regex": "pattern"}),
        };

        let tool_result1 = ToolResult::new(ToolName::new("forge_tool_fs_read"))
            .call_id(ToolCallId::new("call_123"))
            .success(json!({"content": "File content 1"}).to_string());

        let tool_result2 = ToolResult::new(ToolName::new("forge_tool_fs_search"))
            .call_id(ToolCallId::new("call_456"))
            .success(json!({"matches": ["match1", "match2"]}).to_string());

        let context = Context::default()
            .add_message(ContextMessage::user("User message 1"))
            .add_message(ContextMessage::assistant(
                "Assistant message with tool call",
                Some(vec![tool_call1.clone()]),
            ))
            .add_message(ContextMessage::assistant(
                "Another assistant message",
                Some(vec![tool_call2.clone()]),
            ))
            .add_message(ContextMessage::assistant("Third assistant message", None))
            .add_message(ContextMessage::tool_result(tool_result1))
            .add_message(ContextMessage::tool_result(tool_result2))
            .add_message(ContextMessage::user("User message 2"));

        // With the updated logic, we include all messages from first assistant through
        // the end
        let sequence = find_sequence(&context, 0);

        let (start, end) = sequence.unwrap();
        assert_eq!(start, 1); // First assistant message
        assert_eq!(end, 6); // Last message in context excluding the
                            // preservation window
    }

    #[test]
    fn test_only_tool_results() {
        // Test when we have just tool results in sequence
        let tool_result1 = ToolResult::new(ToolName::new("forge_tool_fs_read"))
            .call_id(ToolCallId::new("call_123"))
            .success(json!({"content": "File content 1"}).to_string());

        let tool_result2 = ToolResult::new(ToolName::new("forge_tool_fs_search"))
            .call_id(ToolCallId::new("call_456"))
            .success(json!({"matches": ["match1", "match2"]}).to_string());

        let context = Context::default()
            .add_message(ContextMessage::user("User message 1"))
            .add_message(ContextMessage::tool_result(tool_result1))
            .add_message(ContextMessage::tool_result(tool_result2))
            .add_message(ContextMessage::user("User message 2"));

        // With the updated logic, tool results by themselves are not valid for
        // compaction since they don't start with an assistant message
        let sequence = find_sequence(&context, 0);
        assert!(sequence.is_none());
    }

    #[test]
    fn test_mixed_assistant_and_single_tool() {
        // Create a context with an assistant message and a tool result that are not
        // directly connected
        let tool_call = ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path"}),
        };

        let tool_result = ToolResult::new(ToolName::new("forge_tool_fs_read"))
            .call_id(ToolCallId::new("call_123"))
            .success(json!({"content": "File content 1"}).to_string());

        let context = Context::default()
            .add_message(ContextMessage::user("User message 1")) // 0
            .add_message(ContextMessage::assistant(
                "Assistant message with tool call",
                Some(vec![tool_call]),
            )) // 1
            .add_message(ContextMessage::user("User intermediate message")) // 2
            .add_message(ContextMessage::tool_result(tool_result)) // 3
            .add_message(ContextMessage::user("User message 2")); // 4

        // With the updated compaction logic, we need 2+ messages after the first
        // assistant message This test has 4 messages after the first assistant
        // message (indices 1-4)
        let sequence = find_sequence(&context, 0);

        let (start, end) = sequence.unwrap();
        assert_eq!(start, 1); // First assistant message
        assert_eq!(end, 4); // Last message in context
    }
    #[test]
    fn test_preserve_last_n_messages() {
        // Create a context with multiple sequences that could be compressed
        let context = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message 1"))
            .add_message(ContextMessage::assistant("Assistant message 1", None)) // 2
            .add_message(ContextMessage::assistant("Assistant message 2", None)) // 3
            .add_message(ContextMessage::assistant("Assistant message 3", None)) // 4
            .add_message(ContextMessage::user("User message 2")) // 5
            .add_message(ContextMessage::assistant("Assistant message 4", None)) // 6
            .add_message(ContextMessage::assistant("Assistant message 5", None)); // 7

        // With the updated logic, we should compact from the first assistant message
        // through the end of the context (respecting preservation window)
        let sequence = find_sequence(&context, 0);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 7); // Now includes all messages to the end

        // With preserve_last_n = 3, we should preserve the last 3 messages (indices 5,
        // 6, 7) So we should get a sequence from 2 to 4
        let sequence = find_sequence(&context, 3);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 4);

        // With preserve_last_n = 5, we should preserve indices 3-7
        // So we should get no compressible sequence, since we can only consider indices
        // 0-2
        let sequence = find_sequence(&context, 5);
        assert!(sequence.is_none());

        // With preserve_last_n = 8 (more than total messages), we should get no
        // compressible sequence
        let sequence = find_sequence(&context, 8);
        assert!(sequence.is_none());
    }
    #[test]
    fn test_preserve_last_n_with_sequence_at_end() {
        // Create a context with a sequence at the end
        let context = Context::default()
            .add_message(ContextMessage::system("System message")) // 0
            .add_message(ContextMessage::user("User message 1")) // 1
            .add_message(ContextMessage::assistant("Assistant message 1", None)) // 2
            .add_message(ContextMessage::user("User message 2")) // 3
            .add_message(ContextMessage::assistant("Assistant message 2", None)) // 4
            .add_message(ContextMessage::assistant("Assistant message 3", None)) // 5
            .add_message(ContextMessage::assistant("Assistant message 4", None)); // 6

        // With the updated logic, we should compact from the first assistant message
        // through the end (respecting preservation window)
        let sequence = find_sequence(&context, 0);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2); // First assistant message
        assert_eq!(end, 6); // Last message

        // With preserve_last_n = 2, we should preserve the last 2 messages (indices
        // 5-6) So we would compact from first assistant (index 2) to index 4
        let sequence = find_sequence(&context, 2);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 4);

        // With preserve_last_n = 1, we should preserve index 6
        // So we should compact from index 2 to index 5
        let sequence = find_sequence(&context, 1);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 5);
    }

    #[test]
    fn test_preserve_tool_call_atomicity() {
        let tool_calls = Some(vec![ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: None,
            arguments: json!({"path": "/test/path"}),
        }]);

        let tool_results = vec![ToolResult::new(ToolName::new("forge_tool_fs_read"))
            .call_id(ToolCallId::new("call_123"))
            .success(json!({"content": "File content 1"}).to_string())];

        // Create a context with a sequence at the end
        let context = Context::default()
            .add_message(ContextMessage::system("System message")) // 0
            .add_message(ContextMessage::user("User Message 1")) // 1
            .add_message(ContextMessage::assistant(
                "Assistant Message 1",
                tool_calls.clone(),
            )) // 2
            .add_tool_results(tool_results.clone()) // 3
            .add_message(ContextMessage::assistant(
                "Assistant Message 2",
                tool_calls.clone(),
            )) // 4
            .add_tool_results(tool_results.clone()) // 5
            .add_message(ContextMessage::assistant(
                "Assistant Message 3",
                tool_calls.clone(),
            )) // 6
            .add_tool_results(tool_results.clone()) // 7
            .add_message(ContextMessage::assistant(
                "Assistant Message 4",
                tool_calls.clone(),
            )) // 8
            .add_tool_results(tool_results.clone()); // 9

        // All the messages should be considered
        let sequence = find_sequence(&context, 0).unwrap();
        assert_eq!(sequence, (2, 9));

        // Since we can not break in between a tool call is corresponding tool-result
        let sequence = find_sequence(&context, 1).unwrap();
        assert_eq!(sequence, (2, 7));

        let sequence = find_sequence(&context, 2).unwrap();
        assert_eq!(sequence, (2, 7));
    }

    #[test]
    fn test_conversation_compaction_from_first_assistant_to_last() {
        // Create a context with a mixed conversation including user and assistant
        // messages
        let context = Context::default()
            .add_message(ContextMessage::system("System message")) // 0
            .add_message(ContextMessage::user("Initial user request")) // 1
            .add_message(ContextMessage::assistant("Assistant response 1", None)) // 2
            .add_message(ContextMessage::user("User follow-up question")) // 3
            .add_message(ContextMessage::assistant("Assistant response 2", None)) // 4
            .add_message(ContextMessage::user("Another user question")) // 5
            .add_message(ContextMessage::assistant("Assistant response 3", None)) // 6
            .add_message(ContextMessage::user("Final user question")) // 7
            .add_message(ContextMessage::assistant("Assistant response 4", None)); // 8

        // With no preservation, we should compact from the first assistant message
        // (index 2) to the last message (index 8)
        let sequence = find_sequence(&context, 0);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 8);

        // With preserve_last_n = 2, we should preserve the last 2 messages (indices
        // 7-8) So we should compact from index 2 to index 6
        let sequence = find_sequence(&context, 2);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 6);

        // With preserve_last_n = 6, we should preserve the last 6 messages (indices
        // 3-8) So we would compact from first assistant (index 2) to index 2,
        // but since that's just one message, no effective sequence is found
        let sequence = find_sequence(&context, 6);
        // With the updated logic, we still get a valid compaction sequence
        // but it's just a single message which isn't enough to compact effectively
        assert!(sequence.is_none());
    }

    #[test]
    fn test_conversation_with_mixed_message_types() {
        // Create a context with a mixed conversation including user messages, assistant
        // messages, tool calls, and tool results
        let tool_call = ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path"}),
        };

        let tool_result = ToolResult::new(ToolName::new("forge_tool_fs_read"))
            .call_id(ToolCallId::new("call_123"))
            .success(json!({"content": "File content"}).to_string());

        let context = Context::default()
            .add_message(ContextMessage::system("System message")) // 0
            .add_message(ContextMessage::user("Initial user request")) // 1
            .add_message(ContextMessage::assistant(
                "Assistant response with tool call",
                Some(vec![tool_call.clone()]),
            )) // 2
            .add_message(ContextMessage::tool_result(tool_result.clone())) // 3
            .add_message(ContextMessage::user("User follow-up")) // 4
            .add_message(ContextMessage::assistant("Assistant response", None)) // 5
            .add_message(ContextMessage::user("Another question")) // 6
            .add_message(ContextMessage::assistant(
                "Another assistant response with tool call",
                Some(vec![tool_call.clone()]),
            )) // 7
            .add_message(ContextMessage::tool_result(tool_result.clone())); // 8

        // With no preservation, we should compact from the first assistant message
        // (index 2) to the last message (index 8)
        let sequence = find_sequence(&context, 0);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 8);

        // With preserve_last_n = 3, we should preserve the last 3 messages (indices
        // 6-8) So we should compact from index 2 to index 5
        let sequence = find_sequence(&context, 3);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 5);
    }

    #[test]
    fn test_first_message_is_assistant() {
        // Test case where the first message is from the assistant (after system
        // message)
        let context = Context::default()
            .add_message(ContextMessage::system("System message")) // 0
            .add_message(ContextMessage::assistant("First assistant message", None)) // 1
            .add_message(ContextMessage::user("User response")) // 2
            .add_message(ContextMessage::assistant("Second assistant message", None)); // 3

        // With no preservation, we should compact from index 1 to index 3
        let sequence = find_sequence(&context, 0);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 1);
        assert_eq!(end, 3);
    }

    #[test]
    fn test_assistant_message_with_tool_call_at_end() {
        // Test case where the last message has a tool call and needs special handling
        let tool_call = ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path"}),
        };

        let context = Context::default()
            .add_message(ContextMessage::system("System message")) // 0
            .add_message(ContextMessage::user("Initial user request")) // 1
            .add_message(ContextMessage::assistant("First assistant message", None)) // 2
            .add_message(ContextMessage::user("User follow-up")) // 3
            .add_message(ContextMessage::assistant(
                "Assistant response with tool call",
                Some(vec![tool_call.clone()]),
            )); // 4

        // With no preservation, we should get a compaction range but exclude the tool
        // call message since it wouldn't have a corresponding tool result
        let sequence = find_sequence(&context, 0);
        let (start, end) = sequence.unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 3);
    }
    #[test]
    fn test_empty_context() {
        // Test edge case: an empty context
        let context = Context::default();
        let result = find_sequence(&context, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_single_message_context() {
        // Test edge case: context with only one message
        let context = Context::default().add_message(ContextMessage::system("System message"));
        let result = find_sequence(&context, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_preserve_equals_length() {
        // Test edge case: preservation window equals message count
        let context = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message"))
            .add_message(ContextMessage::assistant("Assistant message", None));

        // Context has 3 messages, preserve_last_n = 3
        let result = find_sequence(&context, 3);
        assert!(result.is_none());
    }

    #[test]
    fn test_max_len_zero_after_tool_call() {
        // Test edge case: max_len becomes 0 after tool call adjustment
        // Create a context with 2 messages where the second one has a tool call
        let tool_call = ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path"}),
        };

        let context = Context::default()
            .add_message(ContextMessage::user("User message"))
            .add_message(ContextMessage::assistant(
                "Assistant message with tool call",
                Some(vec![tool_call]),
            ));

        // With preserve_last_n = 0, max_len = 2, but after tool call adjustment it
        // could become 1 which might lead to underflow in some parts of the
        // code
        let result = find_sequence(&context, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_start_end_positions() {
        // Test edge case: empty start/end positions
        // Create a context with only system and user messages (no assistant messages)
        // which would result in empty start/end position vectors
        let context = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message 1"))
            .add_message(ContextMessage::user("User message 2"))
            .add_message(ContextMessage::user("User message 3"));

        let result = find_sequence(&context, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_potential_underflow_edge_cases() {
        // Test edge case: potential integer underflow scenarios

        // Case 1: preserve_last_n = 1, total messages = 2, with the last message having
        // a tool call
        let tool_call = ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path"}),
        };

        let context = Context::default()
            .add_message(ContextMessage::user("User message"))
            .add_message(ContextMessage::assistant(
                "Assistant message with tool call",
                Some(vec![tool_call]),
            ));

        // With preserve_last_n = 1, max_len = 2-1 = 1,
        // then if we try to check messages[max_len-1] this could cause underflow if not
        // handled
        let result = find_sequence(&context, 1);
        assert!(result.is_none());

        // Case 2: Context with exactly 2 messages (user, assistant)
        let context = Context::default()
            .add_message(ContextMessage::user("User message"))
            .add_message(ContextMessage::assistant("Assistant message", None));

        // With preserve_last_n = 0, max_len = 2, but we need at least 3 messages for
        // compression
        let result = find_sequence(&context, 0);
        assert!(result.is_none());
    }
}
