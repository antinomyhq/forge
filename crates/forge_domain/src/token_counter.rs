use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::message::ChatCompletionMessage;

/// Represents a count of characters in text content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharCount(usize);

impl CharCount {
    pub fn value(&self) -> usize {
        self.0
    }
}

impl Deref for CharCount {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<usize> for CharCount {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl std::ops::Add for CharCount {
    type Output = CharCount;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.value() + rhs.value())
    }
}

/// Represents a count of tokens, typically derived from character count
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TokenCount(usize);

impl TokenCount {
    pub fn value(&self) -> usize {
        self.0
    }
}

impl Deref for TokenCount {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<CharCount> for TokenCount {
    fn from(value: CharCount) -> Self {
        Self(TokenCounter::count_tokens_char_count(value.value()))
    }
}

impl std::fmt::Display for TokenCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Utility for counting tokens in text content
pub struct TokenCounter;

impl TokenCounter {
    /// The ratio of characters to tokens (4 characters = 1 token approximately)
    pub const TOKEN_TO_CHAR_RATIO: usize = 4;

    /// Estimates the number of tokens in the input content.
    /// Currently uses a simple heuristic: content length / TOKEN_TO_CHAR_RATIO
    ///
    /// Rounds up to the nearest multiple of 10 to avoid giving users a false
    /// sense of precision.
    pub fn count_tokens(content: &str) -> usize {
        Self::count_tokens_char_count(content.len())
    }

    fn count_tokens_char_count(count: usize) -> usize {
        (count / Self::TOKEN_TO_CHAR_RATIO + 5) / 10 * 10
    }

    pub const fn token_to_chars(token: usize) -> usize {
        token * Self::TOKEN_TO_CHAR_RATIO
    }
}

/// A trait for types that represent some number of characters (aka bytes).
/// For use in calculating context window size utilization.
pub trait CharCounter {
    /// Returns the number of characters contained within this type.
    ///
    /// One "character" is essentially the same as one "byte"
    fn char_count(&self) -> CharCount;
}

impl CharCounter for String {
    fn char_count(&self) -> CharCount {
        self.len().into()
    }
}

impl CharCounter for &str {
    fn char_count(&self) -> CharCount {
        self.len().into()
    }
}

impl CharCounter for ChatCompletionMessage {
    fn char_count(&self) -> CharCount {
        let mut total_chars = 0;

        // Count content characters
        if let Some(ref content) = self.content {
            total_chars += content.as_str().len();
        }

        // Count reasoning characters
        if let Some(ref reasoning) = self.reasoning {
            total_chars += reasoning.as_str().len();
        }

        // Count tool call characters
        for tool_call in &self.tool_calls {
            match tool_call {
                crate::ToolCall::Full(full) => {
                    total_chars += full.name.as_str().len();
                    total_chars += calculate_value_char_count(&full.arguments);
                }
                crate::ToolCall::Part(part) => {
                    if let Some(ref name) = part.name {
                        total_chars += name.as_str().len();
                    }
                    total_chars += part.arguments_part.len();
                }
            }
        }

        total_chars.into()
    }
}

/// Reflects a detailed accounting of the context window utilization for a given
/// conversation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConversationSize {
    pub system_messages: CharCount,
    pub user_messages: CharCount,
    pub assistant_messages: CharCount,
}

impl CharCounter for ConversationSize {
    fn char_count(&self) -> CharCount {
        self.user_messages + self.assistant_messages + self.system_messages
    }
}

/// Calculate character count for JSON values (used for tool arguments)
fn calculate_value_char_count(document: &serde_json::Value) -> usize {
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_token_count() {
        let fixture_text = "This is a test sentence.";
        let actual_count = TokenCounter::count_tokens(fixture_text);
        let expected_count = (fixture_text.len() / 4 + 5) / 10 * 10;
        assert_eq!(actual_count, expected_count);
    }

    #[test]
    fn test_char_count_operations() {
        let fixture_count1 = CharCount::from(100);
        let fixture_count2 = CharCount::from(200);
        let actual_sum = fixture_count1 + fixture_count2;
        let expected_sum = CharCount::from(300);
        assert_eq!(actual_sum, expected_sum);
    }

    #[test]
    fn test_token_count_from_char_count() {
        let fixture_char_count = CharCount::from(400);
        let actual_token_count: TokenCount = fixture_char_count.into();
        let expected_token_count = TokenCount(100); // 400 / 4 = 100, rounded to nearest 10
        assert_eq!(actual_token_count, expected_token_count);
    }

    #[test]
    fn test_calculate_value_char_count() {
        // Test simple types
        let fixture_string = serde_json::Value::String("hello".to_string());
        let actual_count = calculate_value_char_count(&fixture_string);
        let expected_count = 5;
        assert_eq!(actual_count, expected_count);

        let fixture_number = serde_json::Value::Number(serde_json::Number::from(123));
        let actual_count = calculate_value_char_count(&fixture_number);
        let expected_count = 1;
        assert_eq!(actual_count, expected_count);

        let fixture_bool = serde_json::Value::Bool(true);
        let actual_count = calculate_value_char_count(&fixture_bool);
        let expected_count = 1;
        assert_eq!(actual_count, expected_count);

        let fixture_null = serde_json::Value::Null;
        let actual_count = calculate_value_char_count(&fixture_null);
        let expected_count = 1;
        assert_eq!(actual_count, expected_count);
    }

    #[test]
    fn test_calculate_value_char_count_complex() {
        // Test array
        let fixture_array = serde_json::Value::Array(vec![
            serde_json::Value::String("test".to_string()),
            serde_json::Value::Number(serde_json::Number::from(42)),
            serde_json::Value::Bool(false),
        ]);
        let actual_count = calculate_value_char_count(&fixture_array);
        let expected_count = 6; // "test" (4) + Number (1) + Bool (1)
        assert_eq!(actual_count, expected_count);

        // Test object
        let mut fixture_obj = serde_json::Map::new();
        fixture_obj.insert(
            "key1".to_string(),
            serde_json::Value::String("value1".to_string()),
        );
        fixture_obj.insert(
            "key2".to_string(),
            serde_json::Value::Number(serde_json::Number::from(99)),
        );
        let fixture_object = serde_json::Value::Object(fixture_obj);
        let actual_count = calculate_value_char_count(&fixture_object);
        let expected_count = 7; // "value1" (6) + Number (1)
        assert_eq!(actual_count, expected_count);
    }
}
