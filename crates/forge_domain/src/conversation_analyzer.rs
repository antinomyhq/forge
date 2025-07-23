use crate::token_counter::{CharCount, CharCounter, ConversationSize, TokenCount};
use crate::{Context, ContextMessage, Conversation, Role};

/// Calculate character count for JSON values (used for tool arguments)
/// This is a public version of the function from token_counter
pub fn calculate_value_char_count(document: &serde_json::Value) -> usize {
    match document {
        serde_json::Value::Null => 1,
        serde_json::Value::Bool(_) => 1,
        serde_json::Value::Number(_) => 1,
        serde_json::Value::String(s) => s.len(),
        serde_json::Value::Array(vec) => vec
            .iter()
            .fold(0, |acc, v| acc + calculate_value_char_count(v)),
        serde_json::Value::Object(map) => map
            .values()
            .fold(0, |acc, v| acc + calculate_value_char_count(v)),
    }
}

/// Analyzer for calculating conversation token usage
pub struct ConversationAnalyzer;

impl ConversationAnalyzer {
    /// Calculate conversation size by analyzing the context messages
    pub fn calculate_conversation_size(context: &Context) -> ConversationSize {
        let mut user_chars = 0;
        let mut assistant_chars = 0;
        let mut context_chars = 0;

        for message in &context.messages {
            match message {
                ContextMessage::Text(text_message) => {
                    let char_count = text_message.char_count();
                    match text_message.role {
                        Role::User => user_chars += char_count.value(),
                        Role::Assistant => assistant_chars += char_count.value(),
                        Role::System => context_chars += char_count.value(),
                    }
                }
                ContextMessage::Tool(tool_result) => {
                    // Tool results are considered part of context
                    context_chars += tool_result.char_count().value();
                }
                ContextMessage::Image(_) => {
                    // Images are considered user content
                    user_chars += 1000; // Approximate token count for images
                }
            }
        }

        ConversationSize {
            context_messages: context_chars.into(),
            user_messages: user_chars.into(),
            assistant_messages: assistant_chars.into(),
        }
    }

    /// Calculate detailed token breakdown for usage display
    /// Returns (context_tokens, user_tokens, assistant_tokens, tools_tokens,
    /// total_tokens)
    pub fn calculate_detailed_breakdown(
        conversation: &Conversation,
    ) -> (usize, usize, usize, usize, usize) {
        let mut context_tokens = 0;
        let mut user_tokens = 0;
        let mut assistant_tokens = 0;
        let mut tools_tokens = 0;

        // Calculate from conversation context if available
        if let Some(ref context) = conversation.context {
            let size = Self::calculate_conversation_size(context);
            context_tokens = TokenCount::from(size.context_messages).value();
            user_tokens = TokenCount::from(size.user_messages).value();
            assistant_tokens = TokenCount::from(size.assistant_messages).value();

            // Calculate tools tokens from tool definitions
            tools_tokens = context
                .tools
                .iter()
                .map(|tool| {
                    let tool_json = serde_json::to_string(tool).unwrap_or_default();
                    tool_json.char_count().value() / 4 // Convert to approximate tokens
                })
                .sum();
        }

        let total_tokens = context_tokens + user_tokens + assistant_tokens + tools_tokens;
        (
            context_tokens,
            user_tokens,
            assistant_tokens,
            tools_tokens,
            total_tokens,
        )
    }
}

impl CharCounter for crate::TextMessage {
    fn char_count(&self) -> CharCount {
        let mut total_chars = 0;

        // Count content characters
        total_chars += self.content.len();

        // Count tool call characters
        if let Some(ref tool_calls) = self.tool_calls {
            for tool_call in tool_calls {
                total_chars += tool_call.name.as_str().len();
                total_chars += calculate_value_char_count(&tool_call.arguments);
            }
        }

        // Count reasoning details characters
        if let Some(ref reasoning_details) = self.reasoning_details {
            for detail in reasoning_details {
                if let Some(ref text) = detail.text {
                    total_chars += text.len();
                }
            }
        }

        total_chars.into()
    }
}

impl CharCounter for crate::ToolResult {
    fn char_count(&self) -> CharCount {
        let mut total_chars = 0;

        // Count tool name
        total_chars += self.name.as_str().len();

        // Count output values
        for value in &self.output.values {
            total_chars += match value {
                crate::ToolValue::Text(text) => text.len(),
                crate::ToolValue::Image(_) => 1000, // Approximate for images
                crate::ToolValue::Empty => 0,
            };
        }

        total_chars.into()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{ContextMessage, Role, TextMessage, ToolName, ToolOutput, ToolResult, ToolValue};

    #[test]
    fn test_calculate_conversation_size_empty_context() {
        let fixture_context = Context::default();
        let actual_size = ConversationAnalyzer::calculate_conversation_size(&fixture_context);
        let expected_size = ConversationSize {
            context_messages: CharCount::from(0),
            user_messages: CharCount::from(0),
            assistant_messages: CharCount::from(0),
        };
        assert_eq!(actual_size.context_messages, expected_size.context_messages);
        assert_eq!(actual_size.user_messages, expected_size.user_messages);
        assert_eq!(
            actual_size.assistant_messages,
            expected_size.assistant_messages
        );
    }

    #[test]
    fn test_calculate_conversation_size_with_messages() {
        let fixture_context = Context::default()
            .add_message(ContextMessage::system("System message"))
            .add_message(ContextMessage::user("User message", None))
            .add_message(ContextMessage::assistant("Assistant message", None, None));

        let actual_size = ConversationAnalyzer::calculate_conversation_size(&fixture_context);

        // System message should be counted as context
        assert!(actual_size.context_messages.value() > 0);
        // User message should be counted as user
        assert!(actual_size.user_messages.value() > 0);
        // Assistant message should be counted as assistant
        assert!(actual_size.assistant_messages.value() > 0);
    }

    #[test]
    fn test_calculate_detailed_breakdown_empty_conversation() {
        let fixture_conversation = crate::Conversation::new(
            crate::ConversationId::generate(),
            crate::Workflow::default(),
            vec![],
        );

        let (context_tokens, user_tokens, assistant_tokens, tools_tokens, total_tokens) =
            ConversationAnalyzer::calculate_detailed_breakdown(&fixture_conversation);

        let expected_total = context_tokens + user_tokens + assistant_tokens + tools_tokens;
        assert_eq!(total_tokens, expected_total);
    }

    #[test]
    fn test_text_message_char_count() {
        let fixture_message = TextMessage {
            role: Role::User,
            content: "Hello world".to_string(),
            tool_calls: None,
            reasoning_details: None,
            model: None,
        };

        let actual_count = fixture_message.char_count();
        let expected_count = CharCount::from(11); // "Hello world" = 11 chars
        assert_eq!(actual_count, expected_count);
    }

    #[test]
    fn test_tool_result_char_count() {
        let fixture_result = ToolResult {
            name: ToolName::new("test_tool"),
            call_id: None,
            output: ToolOutput {
                values: vec![ToolValue::Text("Test output".to_string())],
                is_error: false,
            },
        };

        let actual_count = fixture_result.char_count();
        // "test_tool" (9) + "Test output" (11) = 20 chars
        let expected_count = CharCount::from(20);
        assert_eq!(actual_count, expected_count);
    }
}
