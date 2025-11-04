use serde_json::json;

use crate::{Context, ContextMessage, ModelId, ToolCallFull, ToolCallId, ToolName, ToolResult};

/// Converts a condensed string pattern into a Context with messages.
///
/// This utility type is primarily used in tests to quickly create Context
/// objects with specific message sequences without verbose setup code.
///
/// # Pattern Format
///
/// Each character in the pattern represents a message with a specific role:
/// - `'u'` = User message
/// - `'a'` = Assistant message
/// - `'s'` = System message
/// - `'t'` = Assistant message with tool call
/// - `'r'` = Tool result message
///
/// # Examples
///
/// ```rust,ignore
/// // Creates: User -> Assistant -> User
/// let context = MessagePattern::new("uau").build();
///
/// // Creates: System -> System -> User -> System -> User -> System -> User -> System -> Assistant -> Assistant -> System -> Assistant
/// let context = MessagePattern::new("ssusususaasa").build();
///
/// // Creates: User -> Assistant with tool call -> Tool result -> User
/// let context = MessagePattern::new("utru").build();
/// ```
#[derive(Debug, Clone)]
pub struct MessagePattern {
    pattern: String,
}

impl MessagePattern {
    /// Creates a new MessagePattern from the given pattern string.
    ///
    /// # Arguments
    ///
    /// * `pattern` - A string where each character represents a message role:
    ///   - `'u'` for User
    ///   - `'a'` for Assistant
    ///   - `'s'` for System
    ///   - `'t'` for Assistant with tool call
    ///   - `'r'` for Tool result
    pub fn new(pattern: impl Into<String>) -> Self {
        Self { pattern: pattern.into() }
    }

    /// Builds a Context from the pattern.
    ///
    /// Each message will have content in the format "Message {index}" where
    /// index starts from 1. Tool calls and tool results use predefined test
    /// data.
    ///
    /// # Panics
    ///
    /// Panics if the pattern contains any character other than 'u', 'a', 's',
    /// 't', or 'r'.
    pub fn build(self) -> Context {
        let model_id = ModelId::new("gpt-4");

        let tool_call = ToolCallFull {
            name: ToolName::new("read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path"}).into(),
        };

        let tool_result = ToolResult::new(ToolName::new("read"))
            .call_id(ToolCallId::new("call_123"))
            .success(json!({"content": "File content"}).to_string());

        let messages: Vec<ContextMessage> = self
            .pattern
            .chars()
            .enumerate()
            .map(|(i, c)| {
                let content = format!("Message {}", i + 1);
                match c {
                    'u' => ContextMessage::user(&content, Some(model_id.clone())),
                    'a' => ContextMessage::assistant(&content, None, None),
                    's' => ContextMessage::system(&content),
                    't' => ContextMessage::assistant(&content, None, Some(vec![tool_call.clone()])),
                    'r' => ContextMessage::tool_result(tool_result.clone()),
                    _ => {
                        panic!("Invalid character '{c}' in pattern. Use 'u', 'a', 's', 't', or 'r'")
                    }
                }
            })
            .collect();
        Context::default().messages(messages)
    }
}

impl From<&str> for MessagePattern {
    fn from(pattern: &str) -> Self {
        Self::new(pattern)
    }
}

impl From<String> for MessagePattern {
    fn from(pattern: String) -> Self {
        Self::new(pattern)
    }
}

/// Parses a pattern string to extract indices where `^` markers appear.
///
/// The pattern uses:
/// - `.` to count positions (increments index)
/// - `^` to mark positions to capture
/// - whitespace to ignore (doesn't affect counting)
///
/// # Arguments
///
/// * `pattern` - A string pattern containing `.` for positions and `^` for
///   markers
///
/// # Returns
///
/// A vector of indices where `^` markers were found
///
/// # Examples
///
/// ```ignore
/// let indices = index_from_pattern("...^..^");
/// assert_eq!(indices, vec![3, 6]);
///
/// let [first, second] = indices.as_slice();
/// // first = 3, second = 6
/// ```
pub fn index_from_pattern(pattern: impl ToString) -> Vec<usize> {
    let pattern = pattern.to_string();
    let mut indices = Vec::new();
    let mut current_index = 0;

    for ch in pattern.chars() {
        match ch {
            '^' => {
                indices.push(current_index);
                current_index += 1;
            }
            '.' => {
                current_index += 1;
            }
            _ if ch.is_whitespace() => {
                // Ignore whitespace
            }
            _ => {
                // Ignore other characters
            }
        }
    }

    indices
}

/// Creates a pattern string with `^` markers at the specified indices.
///
/// The generated pattern uses:
/// - `.` for positions without markers
/// - `^` for positions that should be marked
///
/// # Arguments
///
/// * `indices` - A slice of indices where `^` markers should be placed
/// * `length` - The total length of the pattern (if None, uses the last index +
///   1)
///
/// # Returns
///
/// A string pattern with `.` and `^` characters
///
/// # Examples
///
/// ```ignore
/// let pattern = pattern_from_indices(&[3, 6], Some(7));
/// assert_eq!(pattern, "...^..^");
///
/// let pattern = pattern_from_indices(&[0, 2], None);
/// assert_eq!(pattern, "^.^");
/// ```
pub fn pattern_from_indices(indices: &[usize], length: Option<usize>) -> String {
    if indices.is_empty() {
        return String::new();
    }

    let max_index = *indices.iter().max().unwrap();
    let pattern_length = length.unwrap_or(max_index + 1);

    let mut pattern = String::with_capacity(pattern_length);
    let indices_set: std::collections::HashSet<usize> = indices.iter().copied().collect();

    for i in 0..pattern_length {
        if indices_set.contains(&i) {
            pattern.push('^');
        } else {
            pattern.push('.');
        }
    }

    pattern
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{ContextMessage, ModelId, Role, TextMessage};

    #[test]
    fn test_message_pattern_single_user() {
        let fixture = MessagePattern::new("u");
        let actual = fixture.build();
        let expected = Context::default().messages(vec![ContextMessage::Text(TextMessage {
            role: Role::User,
            content: "Message 1".to_string(),
            raw_content: None,
            tool_calls: None,
            model: Some(ModelId::new("gpt-4")),
            reasoning_details: None,
        })]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_message_pattern_user_assistant_user() {
        let fixture = MessagePattern::new("uau");
        let actual = fixture.build();
        let expected = Context::default().messages(vec![
            ContextMessage::Text(TextMessage {
                role: Role::User,
                content: "Message 1".to_string(),
                raw_content: None,
                tool_calls: None,
                model: Some(ModelId::new("gpt-4")),
                reasoning_details: None,
            }),
            ContextMessage::Text(TextMessage {
                role: Role::Assistant,
                content: "Message 2".to_string(),
                raw_content: None,
                tool_calls: None,
                model: None,
                reasoning_details: None,
            }),
            ContextMessage::Text(TextMessage {
                role: Role::User,
                content: "Message 3".to_string(),
                raw_content: None,
                tool_calls: None,
                model: Some(ModelId::new("gpt-4")),
                reasoning_details: None,
            }),
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_message_pattern_complex() {
        let fixture = MessagePattern::new("ssusususaasa");
        let actual = fixture.build();

        assert_eq!(actual.messages.len(), 12);
        assert!(actual.messages[0].has_role(Role::System));
        assert!(actual.messages[1].has_role(Role::System));
        assert!(actual.messages[2].has_role(Role::User));
        assert!(actual.messages[3].has_role(Role::System));
        assert!(actual.messages[4].has_role(Role::User));
        assert!(actual.messages[5].has_role(Role::System));
        assert!(actual.messages[6].has_role(Role::User));
        assert!(actual.messages[7].has_role(Role::System));
        assert!(actual.messages[8].has_role(Role::Assistant));
        assert!(actual.messages[9].has_role(Role::Assistant));
        assert!(actual.messages[10].has_role(Role::System));
        assert!(actual.messages[11].has_role(Role::Assistant));
    }

    #[test]
    fn test_message_pattern_empty() {
        let fixture = MessagePattern::new("");
        let actual = fixture.build();
        let expected = Context::default();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_message_pattern_all_system() {
        let fixture = MessagePattern::new("sss");
        let actual = fixture.build();

        assert_eq!(actual.messages.len(), 3);
        assert!(actual.messages.iter().all(|m| m.has_role(Role::System)));
    }

    #[test]
    #[should_panic(expected = "Invalid character 'x' in pattern. Use 'u', 'a', 's', 't', or 'r'")]
    fn test_message_pattern_invalid_character() {
        let fixture = MessagePattern::new("uax");
        fixture.build();
    }

    #[test]
    fn test_message_pattern_from_str() {
        let fixture = MessagePattern::from("ua");
        let actual = fixture.build();
        assert_eq!(actual.messages.len(), 2);
    }

    #[test]
    fn test_message_pattern_from_string() {
        let fixture = MessagePattern::from("ua".to_string());
        let actual = fixture.build();
        assert_eq!(actual.messages.len(), 2);
    }

    #[test]
    fn test_message_pattern_content_numbering() {
        let fixture = MessagePattern::new("uau");
        let actual = fixture.build();

        assert_eq!(actual.messages[0].content().unwrap(), "Message 1");
        assert_eq!(actual.messages[1].content().unwrap(), "Message 2");
        assert_eq!(actual.messages[2].content().unwrap(), "Message 3");
    }

    #[test]
    fn test_message_pattern_with_tool_call() {
        let fixture = MessagePattern::new("utr");
        let actual = fixture.build();

        assert_eq!(actual.messages.len(), 3);
        assert!(actual.messages[0].has_role(Role::User));
        assert!(actual.messages[1].has_role(Role::Assistant));
        assert!(actual.messages[1].has_tool_call());
        assert!(actual.messages[2].has_tool_result());
    }

    #[test]
    fn test_message_pattern_with_multiple_tool_calls() {
        let fixture = MessagePattern::new("utrtr");
        let actual = fixture.build();

        assert_eq!(actual.messages.len(), 5);
        assert!(actual.messages[1].has_tool_call());
        assert!(actual.messages[2].has_tool_result());
        assert!(actual.messages[3].has_tool_call());
        assert!(actual.messages[4].has_tool_result());
    }

    #[test]
    fn test_message_pattern_complex_with_tools() {
        let fixture = MessagePattern::new("sutruaua");
        let actual = fixture.build();

        assert_eq!(actual.messages.len(), 8);
        assert!(actual.messages[0].has_role(Role::System));
        assert!(actual.messages[1].has_role(Role::User));
        assert!(actual.messages[2].has_tool_call());
        assert!(actual.messages[3].has_tool_result());
        assert!(actual.messages[4].has_role(Role::User));
        assert!(actual.messages[5].has_role(Role::Assistant));
        assert!(actual.messages[6].has_role(Role::User));
        assert!(actual.messages[7].has_role(Role::Assistant));
    }

    #[test]
    fn test_index_from_pattern() {
        // Basic usage with destructuring
        let indices = index_from_pattern("...^");
        assert_eq!(indices, vec![3]);

        let [first] = indices.as_slice() else {
            panic!("Expected exactly one index");
        };
        assert_eq!(*first, 3);

        // Multiple markers
        let indices = index_from_pattern("...^..^");
        assert_eq!(indices, vec![3, 6]);

        let [first, second] = indices.as_slice() else {
            panic!("Expected exactly two indices");
        };
        assert_eq!(*first, 3);
        assert_eq!(*second, 6);

        // With whitespace (should be ignored)
        let indices = index_from_pattern("  ...^..............^...");
        assert_eq!(indices, vec![3, 18]);

        // Marker at start
        let indices = index_from_pattern("^...");
        assert_eq!(indices, vec![0]);

        // Multiple consecutive markers
        let indices = index_from_pattern(".^^.");
        assert_eq!(indices, vec![1, 2]);

        // Empty pattern
        let indices = index_from_pattern("");
        assert_eq!(indices, Vec::<usize>::new());

        // Only whitespace
        let indices = index_from_pattern("   ");
        assert_eq!(indices, Vec::<usize>::new());

        // Only markers
        let indices = index_from_pattern("^^^");
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_pattern_from_indices() {
        // Basic usage
        let actual = pattern_from_indices(&[3], Some(4));
        let expected = "...^";
        assert_eq!(actual, expected);

        // Multiple indices
        let actual = pattern_from_indices(&[3, 6], Some(7));
        let expected = "...^..^";
        assert_eq!(actual, expected);

        // Marker at start
        let actual = pattern_from_indices(&[0], Some(4));
        let expected = "^...";
        assert_eq!(actual, expected);

        // Multiple consecutive markers
        let actual = pattern_from_indices(&[1, 2], Some(4));
        let expected = ".^^.";
        assert_eq!(actual, expected);

        // Auto-length (no explicit length)
        let actual = pattern_from_indices(&[3, 6], None);
        let expected = "...^..^";
        assert_eq!(actual, expected);

        // Auto-length with single index
        let actual = pattern_from_indices(&[0], None);
        let expected = "^";
        assert_eq!(actual, expected);

        // Empty indices
        let actual = pattern_from_indices(&[], None);
        let expected = "";
        assert_eq!(actual, expected);

        // Longer pattern than needed
        let actual = pattern_from_indices(&[1, 3], Some(10));
        let expected = ".^.^......";
        assert_eq!(actual, expected);

        // Unordered indices (should still work)
        let actual = pattern_from_indices(&[6, 3], Some(7));
        let expected = "...^..^";
        assert_eq!(actual, expected);

        // Duplicate indices (should deduplicate)
        let actual = pattern_from_indices(&[2, 2, 5], Some(7));
        let expected = "..^..^.";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_index_and_pattern_roundtrip() {
        // Test that converting back and forth preserves data
        let original_pattern = "...^..^";
        let indices = index_from_pattern(original_pattern);
        let recreated_pattern = pattern_from_indices(&indices, Some(7));
        assert_eq!(original_pattern, recreated_pattern);

        // Test with auto-length
        let original_pattern = "^.^";
        let indices = index_from_pattern(original_pattern);
        let recreated_pattern = pattern_from_indices(&indices, None);
        assert_eq!(original_pattern, recreated_pattern);

        // Test with whitespace (whitespace is ignored, so we compare indices)
        let pattern_with_whitespace = "  ...^..^";
        let indices = index_from_pattern(pattern_with_whitespace);
        let expected_indices = vec![3, 6];
        assert_eq!(indices, expected_indices);
    }
}
