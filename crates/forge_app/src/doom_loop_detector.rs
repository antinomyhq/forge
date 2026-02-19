use async_trait::async_trait;
use derive_setters::Setters;
use forge_domain::{
    ContextMessage, Conversation, EventData, EventHandle, HandleOperation, Role, TextMessage,
    ToolCallFull, ToolName, ToolcallStartPayload,
};
use tracing::warn;

/// Error returned when a doom loop is detected
#[derive(Debug, thiserror::Error)]
#[error(
    "⚠️  SYSTEM ALERT: You appear to be stuck in a repetitive loop, having made {consecutive_calls} similar calls. This indicates you are not making progress. Please:\n1. Reconsider your approach to solving this problem\n2. Try a different tool or different arguments\n3. If you're stuck, explain what you're trying to accomplish and ask for clarification"
)]
pub struct DoomLoopError {
    pub tool_name: ToolName,
    pub consecutive_calls: usize,
}

/// Detector for identifying doom loops - when tool calls form repetitive
/// patterns
///
/// This detector analyzes conversation history to identify two types of loops:
/// 1. Consecutive identical calls: [A,A,A,A] - same tool with same arguments
/// 2. Repeating patterns: [A,B,C][A,B,C][A,B,C] - sequence of calls repeating
///
/// Both patterns indicate the agent is stuck in a loop, wasting tokens without
/// making progress.
///
/// Can be used as a hook on `on_toolcall_start` events to automatically detect
/// and prevent doom loops during conversation processing.
#[derive(Debug, Clone, Setters)]
pub struct DoomLoopDetector {
    /// Threshold for consecutive identical tool calls before triggering
    /// detection
    threshold: usize,
}

impl Default for DoomLoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl DoomLoopDetector {
    const DEFAULT_THRESHOLD: usize = 3;

    /// Creates a new doom loop detector with the default threshold
    pub fn new() -> Self {
        Self { threshold: Self::DEFAULT_THRESHOLD }
    }

    /// Checks conversation history for doom loops - when the same tool is
    /// called repeatedly with identical arguments
    ///
    /// Returns Some((tool_name, count)) if a doom loop is detected
    ///
    /// # Arguments
    /// * `current_tool_call` - The tool call being executed
    /// * `conversation` - The conversation to analyze for repetitive patterns
    pub fn check_for_doom_loop(
        &self,
        current_tool_call: &ToolCallFull,
        conversation: &Conversation,
    ) -> Option<(ToolName, usize)> {
        // Extract assistant messages from conversation context
        let assistant_messages = conversation
            .context
            .as_ref()
            .map(|ctx| {
                Self::extract_assistant_messages(ctx.messages.iter().map(|entry| &entry.message))
            })
            .unwrap_or_default();

        let current_signature = (
            &current_tool_call.name,
            current_tool_call.arguments.to_owned().into_string(),
        );

        // Check for consecutive identical calls (e.g., [1,1,1,1])
        if let Some(result) =
            self.check_consecutive_identical(&assistant_messages, &current_signature)
        {
            return Some(result);
        }

        // Check for repeating patterns (e.g., [1,2,3][1,2,3][1,2,3])
        self.check_repeating_pattern(&assistant_messages, &current_signature)
    }

    /// Checks for consecutive identical tool calls
    fn check_consecutive_identical(
        &self,
        assistant_messages: &[&TextMessage],
        current_signature: &(&ToolName, String),
    ) -> Option<(ToolName, usize)> {
        // Count consecutive identical tool calls from the end (most recent)
        let mut consecutive_count = 1; // Count includes the current call

        // Iterate through assistant messages in reverse (most recent first)
        for message in assistant_messages.iter().rev() {
            if let Some(tool_calls) = &message.tool_calls {
                // Check if this message contains the same tool call
                let has_matching_call = tool_calls.iter().any(|call| {
                    let signature = (&call.name, call.arguments.to_owned().into_string());
                    signature == *current_signature
                });

                if has_matching_call {
                    consecutive_count += 1;
                } else {
                    // Different tool call found, stop counting
                    break;
                }
            } else {
                // No tool calls in this message, stop counting
                break;
            }
        }

        if consecutive_count >= self.threshold {
            Some((current_signature.0.clone(), consecutive_count))
        } else {
            None
        }
    }

    /// Checks for repeating patterns of tool calls (e.g.,
    /// [1,2,3][1,2,3][1,2,3])
    ///
    /// This detects when a sequence of tool calls is repeated multiple times,
    /// indicating the agent is stuck in a loop even if individual calls differ.
    ///
    /// The detector looks for the most recent repeating pattern by checking
    /// patterns of various lengths working backwards from the current call.
    fn check_repeating_pattern(
        &self,
        assistant_messages: &[&TextMessage],
        current_signature: &(&ToolName, String),
    ) -> Option<(ToolName, usize)> {
        // Extract all tool call signatures from messages in chronological order
        let mut all_signatures: Vec<(ToolName, String)> = assistant_messages
            .iter()
            .filter_map(|msg| msg.tool_calls.as_ref())
            .flat_map(|calls| calls.iter())
            .map(|call| (call.name.clone(), call.arguments.to_owned().into_string()))
            .collect();

        // Add the current call
        all_signatures.push((current_signature.0.clone(), current_signature.1.clone()));

        // Need at least threshold signatures to detect a pattern
        if all_signatures.len() < self.threshold {
            return None;
        }

        // Try different pattern lengths (from 1 to total_signatures - 1)
        // We need at least one repetition to detect a pattern
        // We try from smallest to largest to detect the most granular pattern
        for pattern_length in 1..(all_signatures.len()) {
            let complete_repetitions =
                self.count_recent_pattern_repetitions(&all_signatures, pattern_length);

            // Only trigger if we have at least 'threshold' complete repetitions
            if complete_repetitions >= self.threshold {
                // Return the first tool in the pattern as the representative
                // The pattern starts from the end, so get it from the appropriate position
                let pattern_offset = complete_repetitions.saturating_mul(pattern_length);
                let pattern_start_idx = all_signatures.len().saturating_sub(pattern_offset);
                return Some((
                    all_signatures[pattern_start_idx].0.clone(),
                    complete_repetitions,
                ));
            }
        }

        None
    }

    /// Counts how many times a pattern of given length repeats at the END of
    /// the sequence
    ///
    /// This works backwards from the most recent calls to find repeating
    /// patterns, which allows detecting new patterns even if earlier
    /// patterns existed. For example, in [1,2,3,1,2,3,4,5,4,5,4,5], this
    /// will detect [4,5] repeating 3 times.
    fn count_recent_pattern_repetitions(
        &self,
        signatures: &[(ToolName, String)],
        pattern_length: usize,
    ) -> usize {
        if pattern_length == 0 || signatures.len() < pattern_length {
            return 0;
        }

        // Start from the end and work backwards
        let total_len = signatures.len();
        let mut repetitions = 0;

        // The pattern is defined by the last pattern_length elements
        // For a partial match, we consider it as the start of a new repetition
        let mut check_len = total_len;

        // Special case: if total length is not evenly divisible by pattern_length,
        // we have a partial match at the end
        if !total_len.is_multiple_of(pattern_length) {
            let partial_len = total_len % pattern_length;
            // Check if the partial segment matches the start of what would be the pattern
            // We need to look back to find what the pattern would be
            if total_len >= pattern_length + partial_len {
                let pattern_start = total_len - partial_len - pattern_length;
                let pattern = &signatures[pattern_start..pattern_start + pattern_length];
                let partial = &signatures[total_len - partial_len..];

                if partial == &pattern[..partial_len] {
                    repetitions += 1;
                    check_len = total_len - partial_len;
                } else {
                    // Partial doesn't match, no pattern
                    return 0;
                }
            } else {
                // Not enough data for a pattern
                return 0;
            }
        }

        // Now check complete repetitions working backwards
        if check_len < pattern_length {
            return repetitions;
        }

        // The pattern is the last complete chunk
        let pattern_start = check_len - pattern_length;
        let pattern = &signatures[pattern_start..check_len];
        repetitions += 1; // Count the pattern itself

        // Check backwards for more repetitions
        let mut pos = pattern_start;
        while pos >= pattern_length {
            pos -= pattern_length;
            let chunk = &signatures[pos..pos + pattern_length];
            if chunk == pattern {
                repetitions += 1;
            } else {
                // Pattern broken, stop counting
                break;
            }
        }

        repetitions
    }

    /// Extracts assistant messages from context messages
    ///
    /// Helper method to filter assistant messages from a conversation context
    pub fn extract_assistant_messages<'a>(
        messages: impl Iterator<Item = &'a ContextMessage> + 'a,
    ) -> Vec<&'a TextMessage> {
        messages
            .filter_map(|msg| {
                if let ContextMessage::Text(text_msg) = msg
                    && text_msg.role == Role::Assistant
                {
                    return Some(text_msg);
                }
                None
            })
            .collect()
    }
}

/// Implementation of EventHandle for DoomLoopDetector
///
/// This allows the detector to be used as a hook on toolcall_start events.
/// When a doom loop is detected, it returns an AgentError which causes
/// the tool execution to be skipped and the error to be returned as a tool
/// result.
#[async_trait]
impl EventHandle<EventData<ToolcallStartPayload>> for DoomLoopDetector {
    async fn handle(
        &self,
        event: &EventData<ToolcallStartPayload>,
        conversation: &mut Conversation,
    ) -> HandleOperation {
        let tool_call = &event.payload.tool_call;

        if let Some((tool_name, consecutive_calls)) =
            self.check_for_doom_loop(tool_call, conversation)
        {
            warn!(
                agent_id = %event.agent.id,
                tool_name = %tool_name,
                consecutive_calls,
                "Doom loop detected: same tool called repeatedly with identical arguments"
            );

            return HandleOperation::agent_error(DoomLoopError { tool_name, consecutive_calls });
        }

        HandleOperation::Continue
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Context, ContextMessage, ConversationId, MessageEntry, ToolCallArguments};
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_assistant_message(tool_call: &ToolCallFull) -> TextMessage {
        TextMessage {
            role: Role::Assistant,
            content: String::new(),
            raw_content: None,
            tool_calls: Some(vec![tool_call.clone()]),
            thought_signature: None,
            model: None,
            reasoning_details: None,
            droppable: false,
        }
    }

    fn create_conversation_with_messages(messages: Vec<TextMessage>) -> Conversation {
        let context_messages: Vec<MessageEntry> = messages
            .into_iter()
            .map(|msg| MessageEntry::from(ContextMessage::Text(msg)))
            .collect();

        let context = Context::default().messages(context_messages);

        Conversation {
            id: ConversationId::generate(),
            title: None,
            context: Some(context),
            metrics: Default::default(),
            metadata: forge_domain::MetaData::new(chrono::Utc::now()),
        }
    }

    #[test]
    fn test_doom_loop_detector_detects_identical_calls() {
        let detector = DoomLoopDetector::new();

        let tool_call = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file.txt"}"#));

        // Build history with repeated calls
        let msg1 = create_assistant_message(&tool_call);
        let msg2 = create_assistant_message(&tool_call);
        let conversation = create_conversation_with_messages(vec![msg1, msg2]);

        // Third call - doom loop detected!
        let actual = detector.check_for_doom_loop(&tool_call, &conversation);
        let expected = Some((ToolName::new("read"), 3));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_doom_loop_detector_no_loop_with_two_calls() {
        let detector = DoomLoopDetector::new();

        let tool_call = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file.txt"}"#));

        // Build history with one call
        let msg1 = create_assistant_message(&tool_call);
        let conversation = create_conversation_with_messages(vec![msg1]);

        // Second call - no loop yet (need 3 for default threshold)
        let actual = detector.check_for_doom_loop(&tool_call, &conversation);
        assert_eq!(actual, None);
    }

    #[test]
    fn test_doom_loop_detector_resets_on_different_arguments() {
        let detector = DoomLoopDetector::new();

        let tool_call_1 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file1.txt"}"#));
        let tool_call_2 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file2.txt"}"#));

        // Build history with two calls of first arguments, then different
        let msg1 = create_assistant_message(&tool_call_1);
        let msg2 = create_assistant_message(&tool_call_1);
        let msg3 = create_assistant_message(&tool_call_2);
        let conversation = create_conversation_with_messages(vec![msg1, msg2, msg3]);

        // Call with first arguments again - should not detect loop
        let actual = detector.check_for_doom_loop(&tool_call_1, &conversation);
        assert_eq!(actual, None);
    }

    #[test]
    fn test_doom_loop_detector_resets_on_different_tool() {
        let detector = DoomLoopDetector::new();

        let tool_call_1 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file.txt"}"#));
        let tool_call_2 = ToolCallFull::new("write")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file.txt"}"#));

        // Build history with two same tool calls, then different tool
        let msg1 = create_assistant_message(&tool_call_1);
        let msg2 = create_assistant_message(&tool_call_1);
        let msg3 = create_assistant_message(&tool_call_2);
        let conversation = create_conversation_with_messages(vec![msg1, msg2, msg3]);

        // Call different tool - should not detect loop
        let actual = detector.check_for_doom_loop(&tool_call_2, &conversation);
        assert_eq!(actual, None);
    }

    #[test]
    fn test_doom_loop_detector_custom_threshold() {
        let detector = DoomLoopDetector::new().threshold(2);

        let tool_call = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file.txt"}"#));

        // Build history with one call
        let msg1 = create_assistant_message(&tool_call);
        let conversation = create_conversation_with_messages(vec![msg1]);

        // Second call - doom loop detected with threshold of 2!
        let actual = detector.check_for_doom_loop(&tool_call, &conversation);
        let expected = Some((ToolName::new("read"), 2));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_doom_loop_detector_empty_history() {
        let detector = DoomLoopDetector::new();

        let tool_call = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file.txt"}"#));

        // Empty history - first call, no loop
        let conversation = create_conversation_with_messages(vec![]);

        let actual = detector.check_for_doom_loop(&tool_call, &conversation);
        assert_eq!(actual, None);
    }

    #[test]
    fn test_extract_assistant_messages() {
        let assistant_msg_1 = TextMessage {
            role: Role::Assistant,
            content: "Response 1".to_string(),
            raw_content: None,
            tool_calls: None,
            thought_signature: None,
            model: None,
            reasoning_details: None,
            droppable: false,
        };

        let user_msg = TextMessage {
            role: Role::User,
            content: "Question".to_string(),
            raw_content: None,
            tool_calls: None,
            thought_signature: None,
            model: None,
            reasoning_details: None,
            droppable: false,
        };

        let assistant_msg_2 = TextMessage {
            role: Role::Assistant,
            content: "Response 2".to_string(),
            raw_content: None,
            tool_calls: None,
            thought_signature: None,
            model: None,
            reasoning_details: None,
            droppable: false,
        };

        let messages = [
            ContextMessage::Text(assistant_msg_1.clone()),
            ContextMessage::Text(user_msg),
            ContextMessage::Text(assistant_msg_2.clone()),
        ];

        let result = DoomLoopDetector::extract_assistant_messages(messages.iter());

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content, "Response 1");
        assert_eq!(result[1].content, "Response 2");
    }

    #[test]
    fn test_doom_loop_detector_detects_repeating_pattern_123_123_123() {
        let detector = DoomLoopDetector::new();

        let tool_call_1 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file1.txt"}"#));
        let tool_call_2 = ToolCallFull::new("write")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file2.txt"}"#));
        let tool_call_3 = ToolCallFull::new("patch")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file3.txt"}"#));

        // Build history with pattern [1,2,3][1,2,3]
        let msg1 = create_assistant_message(&tool_call_1);
        let msg2 = create_assistant_message(&tool_call_2);
        let msg3 = create_assistant_message(&tool_call_3);
        let msg4 = create_assistant_message(&tool_call_1);
        let msg5 = create_assistant_message(&tool_call_2);
        let msg6 = create_assistant_message(&tool_call_3);

        let conversation =
            create_conversation_with_messages(vec![msg1, msg2, msg3, msg4, msg5, msg6]);

        // Current call would complete the third repetition [1,2,3][1,2,3][1,2,3]
        let actual = detector.check_for_doom_loop(&tool_call_1, &conversation);

        // Should detect pattern repetition (3 times)
        assert!(actual.is_some());
        let (tool_name, count) = actual.unwrap();
        assert_eq!(tool_name, ToolName::new("read"));
        assert_eq!(count, 3);
    }

    #[test]
    fn test_doom_loop_detector_detects_repeating_pattern_12_12_12() {
        let detector = DoomLoopDetector::new();

        let tool_call_1 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file1.txt"}"#));
        let tool_call_2 = ToolCallFull::new("write")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file2.txt"}"#));

        // Build history with pattern [1,2][1,2]
        let msg1 = create_assistant_message(&tool_call_1);
        let msg2 = create_assistant_message(&tool_call_2);
        let msg3 = create_assistant_message(&tool_call_1);
        let msg4 = create_assistant_message(&tool_call_2);

        let conversation = create_conversation_with_messages(vec![msg1, msg2, msg3, msg4]);

        // Current call would complete the third repetition [1,2][1,2][1,2]
        let actual = detector.check_for_doom_loop(&tool_call_1, &conversation);

        // Should detect pattern repetition (3 times)
        assert!(actual.is_some());
        let (tool_name, count) = actual.unwrap();
        assert_eq!(tool_name, ToolName::new("read"));
        assert_eq!(count, 3);
    }

    #[test]
    fn test_doom_loop_detector_no_pattern_with_partial_repetition() {
        let detector = DoomLoopDetector::new();

        let tool_call_1 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file1.txt"}"#));
        let tool_call_2 = ToolCallFull::new("write")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file2.txt"}"#));
        let tool_call_3 = ToolCallFull::new("patch")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file3.txt"}"#));

        // Build history with pattern [1,2,3][1,2] - incomplete repetition
        let msg1 = create_assistant_message(&tool_call_1);
        let msg2 = create_assistant_message(&tool_call_2);
        let msg3 = create_assistant_message(&tool_call_3);
        let msg4 = create_assistant_message(&tool_call_1);
        let msg5 = create_assistant_message(&tool_call_2);

        let conversation = create_conversation_with_messages(vec![msg1, msg2, msg3, msg4, msg5]);

        // Current call would not complete a full third repetition
        let actual = detector.check_for_doom_loop(&tool_call_2, &conversation);

        // Should not detect pattern (incomplete)
        assert_eq!(actual, None);
    }

    #[test]
    fn test_doom_loop_detector_pattern_with_custom_threshold() {
        let detector = DoomLoopDetector::new().threshold(2);

        let tool_call_1 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file1.txt"}"#));
        let tool_call_2 = ToolCallFull::new("write")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file2.txt"}"#));

        // Build history with pattern [1,2]
        let msg1 = create_assistant_message(&tool_call_1);
        let msg2 = create_assistant_message(&tool_call_2);

        let conversation = create_conversation_with_messages(vec![msg1, msg2]);

        // Current call would complete the second repetition [1,2][1,2]
        let actual = detector.check_for_doom_loop(&tool_call_1, &conversation);

        // Should detect pattern with threshold of 2
        assert!(actual.is_some());
        let (tool_name, count) = actual.unwrap();
        assert_eq!(tool_name, ToolName::new("read"));
        assert_eq!(count, 2);
    }

    #[test]
    fn test_doom_loop_detector_consecutive_identical_takes_precedence() {
        let detector = DoomLoopDetector::new();

        let tool_call_1 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file1.txt"}"#));

        // Build history with three consecutive identical calls
        let msg1 = create_assistant_message(&tool_call_1);
        let msg2 = create_assistant_message(&tool_call_1);

        let conversation = create_conversation_with_messages(vec![msg1, msg2]);

        // Third consecutive identical call - should be caught by consecutive check
        let actual = detector.check_for_doom_loop(&tool_call_1, &conversation);

        assert!(actual.is_some());
        let (tool_name, count) = actual.unwrap();
        assert_eq!(tool_name, ToolName::new("read"));
        assert_eq!(count, 3);
    }

    #[test]
    fn test_doom_loop_detector_complex_pattern_1234_1234_1234() {
        let detector = DoomLoopDetector::new();

        let tool_call_1 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file1.txt"}"#));
        let tool_call_2 = ToolCallFull::new("write")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file2.txt"}"#));
        let tool_call_3 = ToolCallFull::new("patch")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file3.txt"}"#));
        let tool_call_4 = ToolCallFull::new("shell")
            .arguments(ToolCallArguments::from_json(r#"{"command": "ls"}"#));

        // Build history with pattern [1,2,3,4][1,2,3,4]
        let msg1 = create_assistant_message(&tool_call_1);
        let msg2 = create_assistant_message(&tool_call_2);
        let msg3 = create_assistant_message(&tool_call_3);
        let msg4 = create_assistant_message(&tool_call_4);
        let msg5 = create_assistant_message(&tool_call_1);
        let msg6 = create_assistant_message(&tool_call_2);
        let msg7 = create_assistant_message(&tool_call_3);
        let msg8 = create_assistant_message(&tool_call_4);

        let conversation =
            create_conversation_with_messages(vec![msg1, msg2, msg3, msg4, msg5, msg6, msg7, msg8]);

        // Current call would complete the third repetition [1,2,3,4][1,2,3,4][1,2,3,4]
        let actual = detector.check_for_doom_loop(&tool_call_1, &conversation);

        // Should detect pattern repetition (3 times)
        assert!(actual.is_some());
        let (tool_name, count) = actual.unwrap();
        assert_eq!(tool_name, ToolName::new("read"));
        assert_eq!(count, 3);
    }

    #[test]
    fn test_doom_loop_detector_real_world_scenario() {
        let detector = DoomLoopDetector::new();

        // Simulate a real-world loop: read file, check diagnostics, patch file
        let read_call = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "src/main.rs"}"#));
        let diagnostics_call = ToolCallFull::new("mcp_forge_extension_tool_get_diagnostics")
            .arguments(ToolCallArguments::from_json(r#"{"severity": "error"}"#));
        let patch_call = ToolCallFull::new("patch").arguments(ToolCallArguments::from_json(
            r#"{"path": "src/main.rs", "old": "foo", "new": "bar"}"#,
        ));

        // Create pattern [read, diagnostics, patch] repeated twice
        let msg1 = create_assistant_message(&read_call);
        let msg2 = create_assistant_message(&diagnostics_call);
        let msg3 = create_assistant_message(&patch_call);
        let msg4 = create_assistant_message(&read_call);
        let msg5 = create_assistant_message(&diagnostics_call);
        let msg6 = create_assistant_message(&patch_call);

        let conversation =
            create_conversation_with_messages(vec![msg1, msg2, msg3, msg4, msg5, msg6]);

        // Third iteration begins
        let actual = detector.check_for_doom_loop(&read_call, &conversation);

        // Should detect the pattern loop
        assert!(actual.is_some());
        let (tool_name, count) = actual.unwrap();
        assert_eq!(tool_name, ToolName::new("read"));
        assert_eq!(count, 3);
    }

    #[test]
    fn test_doom_loop_detector_pattern_changes_midway_123123454545() {
        let detector = DoomLoopDetector::new();

        let tool_call_1 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file1.txt"}"#));
        let tool_call_2 = ToolCallFull::new("write")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file2.txt"}"#));
        let tool_call_3 = ToolCallFull::new("patch")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file3.txt"}"#));
        let tool_call_4 = ToolCallFull::new("shell")
            .arguments(ToolCallArguments::from_json(r#"{"command": "ls"}"#));
        let tool_call_5 = ToolCallFull::new("fs_search")
            .arguments(ToolCallArguments::from_json(r#"{"pattern": "test"}"#));

        // Build history with pattern [1,2,3][1,2,3] then [4,5][4,5]
        // Pattern: 123123454545
        let msg1 = create_assistant_message(&tool_call_1);
        let msg2 = create_assistant_message(&tool_call_2);
        let msg3 = create_assistant_message(&tool_call_3);
        let msg4 = create_assistant_message(&tool_call_1);
        let msg5 = create_assistant_message(&tool_call_2);
        let msg6 = create_assistant_message(&tool_call_3);
        let msg7 = create_assistant_message(&tool_call_4);
        let msg8 = create_assistant_message(&tool_call_5);
        let msg9 = create_assistant_message(&tool_call_4);
        let msg10 = create_assistant_message(&tool_call_5);
        let msg11 = create_assistant_message(&tool_call_4);

        let conversation = create_conversation_with_messages(vec![
            msg1, msg2, msg3, msg4, msg5, msg6, msg7, msg8, msg9, msg10, msg11,
        ]);

        // Current call would be the 6th occurrence of tool_call_5, completing
        // [4,5][4,5][4,5]
        let actual = detector.check_for_doom_loop(&tool_call_5, &conversation);

        // Should detect the [4,5][4,5][4,5] pattern at the end
        // The detector looks for the longest repeating pattern, starting from the most
        // recent calls
        assert!(actual.is_some());
        let (tool_name, count) = actual.unwrap();
        // The pattern [4,5] repeats 3 times at the end
        assert_eq!(tool_name, ToolName::new("shell"));
        assert_eq!(count, 3);
    }

    #[test]
    fn test_doom_loop_detector_sequence_1234546454545_step_by_step() {
        let detector = DoomLoopDetector::new();

        // Define the 6 unique tool calls
        let tool_1 = ToolCallFull::new("read")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file1.txt"}"#));
        let tool_2 = ToolCallFull::new("write")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file2.txt"}"#));
        let tool_3 = ToolCallFull::new("patch")
            .arguments(ToolCallArguments::from_json(r#"{"path": "file3.txt"}"#));
        let tool_4 = ToolCallFull::new("shell")
            .arguments(ToolCallArguments::from_json(r#"{"command": "ls"}"#));
        let tool_5 = ToolCallFull::new("fs_search")
            .arguments(ToolCallArguments::from_json(r#"{"pattern": "test"}"#));
        let tool_6 = ToolCallFull::new("sem_search")
            .arguments(ToolCallArguments::from_json(r#"{"queries": []}"#));

        // Sequence: 1234546454545
        // Let's build it step by step and check at each step
        let mut messages = vec![];

        // Step 1: [1] - no loop
        messages.push(create_assistant_message(&tool_1));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_2, &conv), None);

        // Step 2: [1,2] - no loop
        messages.push(create_assistant_message(&tool_2));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_3, &conv), None);

        // Step 3: [1,2,3] - no loop
        messages.push(create_assistant_message(&tool_3));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_4, &conv), None);

        // Step 4: [1,2,3,4] - no loop
        messages.push(create_assistant_message(&tool_4));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_5, &conv), None);

        // Step 5: [1,2,3,4,5] - no loop
        messages.push(create_assistant_message(&tool_5));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_4, &conv), None);

        // Step 6: [1,2,3,4,5,4] - no loop yet
        messages.push(create_assistant_message(&tool_4));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_6, &conv), None);

        // Step 7: [1,2,3,4,5,4,6] - no loop
        messages.push(create_assistant_message(&tool_6));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_4, &conv), None);

        // Step 8: [1,2,3,4,5,4,6,4] - no loop yet
        messages.push(create_assistant_message(&tool_4));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_5, &conv), None);

        // Step 9: [1,2,3,4,5,4,6,4,5] - no loop yet (only 1.5 repetitions of [4,5])
        messages.push(create_assistant_message(&tool_5));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_4, &conv), None);

        // Step 10: [1,2,3,4,5,4,6,4,5,4] - no loop yet (2 repetitions of [4,5])
        messages.push(create_assistant_message(&tool_4));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_5, &conv), None);

        // Step 11: [1,2,3,4,5,4,6,4,5,4,5] - still no loop (2.5 repetitions)
        messages.push(create_assistant_message(&tool_5));
        let conv = create_conversation_with_messages(messages.clone());
        assert_eq!(detector.check_for_doom_loop(&tool_4, &conv), None);

        // Step 12: [1,2,3,4,5,4,6,4,5,4,5,4] - still no loop (almost 3)
        messages.push(create_assistant_message(&tool_4));
        let conv = create_conversation_with_messages(messages.clone());

        // Now the next call (tool_5) would complete the third repetition of [4,5]
        // Current state: ...6,4,5,4,5,4
        // Next call: 5
        // Full pattern at end: 4,5,4,5,4,5 = [4,5] x 3
        let result = detector.check_for_doom_loop(&tool_5, &conv);

        // Should detect pattern [4,5] repeating 3 times
        assert!(result.is_some());
        let (tool_name, count) = result.unwrap();
        assert_eq!(tool_name, ToolName::new("shell")); // First tool in the pattern [4,5]
        assert_eq!(count, 3);
    }
}
