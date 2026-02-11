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
    "⚠️  SYSTEM ALERT: You have called the '{tool_name}' tool {consecutive_calls} times consecutively with identical arguments. This indicates you are stuck in a repetitive loop. Please:\n1. Reconsider your approach to solving this problem\n2. Try a different tool or different arguments\n3. If you're stuck, explain what you're trying to accomplish and ask for clarification"
)]
pub struct DoomLoopError {
    pub tool_name: ToolName,
    pub consecutive_calls: usize,
}

/// Detector for identifying doom loops - when the same tool is called
/// repeatedly with identical arguments
///
/// This detector analyzes conversation history to identify repetitive patterns
/// that indicate the agent is stuck in a loop, wasting tokens without making
/// progress.
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

        // Count consecutive identical tool calls from the end (most recent)
        let mut consecutive_count = 1; // Count includes the current call

        // Iterate through assistant messages in reverse (most recent first)
        for message in assistant_messages.iter().rev() {
            if let Some(tool_calls) = &message.tool_calls {
                // Check if this message contains the same tool call
                let has_matching_call = tool_calls.iter().any(|call| {
                    let signature = (&call.name, call.arguments.to_owned().into_string());
                    signature == current_signature
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
            Some((current_tool_call.name.clone(), consecutive_count))
        } else {
            None
        }
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
}
