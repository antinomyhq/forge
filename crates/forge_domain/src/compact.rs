use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{Context, ModelId, Role};

/// Configuration for automatic context compaction
#[derive(Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option, into)]
pub struct Compact {
    /// Number of most recent messages to preserve during compaction
    /// These messages won't be considered for summarization
    #[merge(strategy = crate::merge::std::overwrite)]
    pub retention_window: usize,

    /// Maximum percentage of the context that can be summarized during
    /// compaction. Valid values are between 0.0 and 1.0, where 0.0 means no
    /// compaction and 1.0 allows summarizing all messages.
    #[merge(strategy = crate::merge::std::overwrite)]
    #[serde(deserialize_with = "deserialize_percentage")]
    pub percentage: f64,

    /// Maximum number of tokens to keep after compaction
    #[merge(strategy = crate::merge::option)]
    pub max_tokens: Option<usize>,

    /// Maximum number of tokens before triggering compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub token_threshold: Option<u64>,

    /// Maximum number of conversation turns before triggering compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub turn_threshold: Option<usize>,

    /// Maximum number of messages before triggering compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub message_threshold: Option<usize>,

    /// Optional custom prompt template to use during compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub prompt: Option<String>,

    /// Model ID to use for compaction, useful when compacting with a
    /// cheaper/faster model
    #[merge(strategy = crate::merge::std::overwrite)]
    pub model: ModelId,
    /// Optional tag name to extract content from when summarizing (e.g.,
    /// "summary")
    #[merge(strategy = crate::merge::std::overwrite)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_tag: Option<SummaryTag>,
}

fn deserialize_percentage<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let value = f64::deserialize(deserializer)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(Error::custom(format!(
            "percentage must be between 0.0 and 1.0, got {value}"
        )));
    }
    Ok(value)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(transparent)]
pub struct SummaryTag(String);

impl Default for SummaryTag {
    fn default() -> Self {
        SummaryTag("forge_context_summary".to_string())
    }
}

impl SummaryTag {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Compact {
    /// Creates a new compaction configuration with the specified maximum token
    /// limit
    pub fn new(model: ModelId) -> Self {
        Self {
            max_tokens: None,
            token_threshold: None,
            turn_threshold: None,
            message_threshold: None,
            prompt: None,
            summary_tag: None,
            model,
            percentage: 0.2, // Default to 20% compaction
            retention_window: 0,
        }
    }

    /// Determines if compaction should be triggered based on the current
    /// context
    pub fn should_compact(&self, context: &Context, token_count: u64) -> bool {
        // Check if any of the thresholds have been exceeded
        if let Some(token_threshold) = self.token_threshold {
            debug!(tokens = ?token_count, "Token count");
            // use provided prompt_tokens if available, otherwise estimate token count
            if token_count >= token_threshold {
                return true;
            }
        }

        if let Some(turn_threshold) = self.turn_threshold {
            if context
                .messages
                .iter()
                .filter(|message| message.has_role(Role::User))
                .count()
                >= turn_threshold
            {
                return true;
            }
        }

        if let Some(message_threshold) = self.message_threshold {
            // Count messages directly from context
            let msg_count = context.messages.len();
            if msg_count >= message_threshold {
                return true;
            }
        }

        false
    }
}

/// Finds a sequence in the context for compaction, starting from the first
/// assistant message and including all messages up to the last possible message
/// (respecting preservation window)
pub fn find_sequence_preserving_last_n(
    context: &Context,
    preserve_last_n: usize,
) -> Option<(usize, usize)> {
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
        .find(|(_, message)| !message.has_role(Role::System))
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::{ContextMessage, ToolCallFull, ToolCallId, ToolName, ToolResult};

    fn seq(pattern: impl ToString, preserve_last_n: usize) -> String {
        let model_id = ModelId::new("gpt-4");
        let pattern = pattern.to_string();

        let tool_call = ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: json!({"path": "/test/path"}),
        };

        let tool_result = ToolResult::new(ToolName::new("forge_tool_fs_read"))
            .call_id(ToolCallId::new("call_123"))
            .success(json!({"content": "File content"}).to_string());

        let mut context = Context::default();

        for c in pattern.chars() {
            match c {
                's' => context = context.add_message(ContextMessage::system("System message")),
                'u' => {
                    context = context.add_message(ContextMessage::user(
                        "User message",
                        model_id.clone().into(),
                    ))
                }
                'a' => {
                    context =
                        context.add_message(ContextMessage::assistant("Assistant message", None))
                }
                't' => {
                    context = context.add_message(ContextMessage::assistant(
                        "Assistant message with tool call",
                        Some(vec![tool_call.clone()]),
                    ))
                }
                'r' => {
                    context = context.add_message(ContextMessage::tool_result(tool_result.clone()))
                }
                _ => panic!("Invalid character in test pattern: {c}"),
            }
        }

        let sequence = find_sequence_preserving_last_n(&context, preserve_last_n);

        let mut result = pattern.clone();
        if let Some((start, end)) = sequence {
            result.insert(start, '[');
            result.insert(end + 2, ']');
        }

        result
    }

    #[test]
    fn test_sequence_finding() {
        // Basic compaction scenarios
        let actual = seq("suaaau", 0);
        let expected = "s[uaaau]";
        assert_eq!(actual, expected);

        let actual = seq("sua", 0);
        let expected = "s[ua]";
        assert_eq!(actual, expected);

        let actual = seq("suauaa", 0);
        let expected = "s[uauaa]";
        assert_eq!(actual, expected);

        // Tool call scenarios
        let actual = seq("suttu", 0);
        let expected = "s[uttu]";
        assert_eq!(actual, expected);

        let actual = seq("sutraau", 0);
        let expected = "s[utraau]";
        assert_eq!(actual, expected);

        let actual = seq("utrutru", 0);
        let expected = "[utrutru]";
        assert_eq!(actual, expected);

        let actual = seq("uttarru", 0);
        let expected = "[uttarru]";
        assert_eq!(actual, expected);

        let actual = seq("urru", 0);
        let expected = "[urru]";
        assert_eq!(actual, expected);

        let actual = seq("uturu", 0);
        let expected = "[uturu]";
        assert_eq!(actual, expected);

        // Preservation window scenarios
        let actual = seq("suaaaauaa", 0);
        let expected = "s[uaaaauaa]";
        assert_eq!(actual, expected);

        let actual = seq("suaaaauaa", 3);
        let expected = "s[uaaaa]uaa";
        assert_eq!(actual, expected);

        let actual = seq("suaaaauaa", 5);
        let expected = "s[uaa]aauaa";
        assert_eq!(actual, expected);

        let actual = seq("suaaaauaa", 8);
        let expected = "suaaaauaa";
        assert_eq!(actual, expected);

        let actual = seq("suauaaa", 0);
        let expected = "s[uauaaa]";
        assert_eq!(actual, expected);

        let actual = seq("suauaaa", 2);
        let expected = "s[uaua]aa";
        assert_eq!(actual, expected);

        let actual = seq("suauaaa", 1);
        let expected = "s[uauaa]a";
        assert_eq!(actual, expected);

        // Tool call atomicity preservation
        let actual = seq("sutrtrtra", 0);
        let expected = "s[utrtrtra]";
        assert_eq!(actual, expected);

        let actual = seq("sutrtrtra", 1);
        let expected = "s[utrtrtr]a";
        assert_eq!(actual, expected);

        let actual = seq("sutrtrtra", 2);
        let expected = "s[utrtr]tra";
        assert_eq!(actual, expected);

        // Conversation patterns
        let actual = seq("suauauaua", 0);
        let expected = "s[uauauaua]";
        assert_eq!(actual, expected);

        let actual = seq("suauauaua", 2);
        let expected = "s[uauaua]ua";
        assert_eq!(actual, expected);

        let actual = seq("suauauaua", 6);
        let expected = "s[ua]uauaua";
        assert_eq!(actual, expected);

        let actual = seq("sutruaua", 0);
        let expected = "s[utruaua]";
        assert_eq!(actual, expected);

        let actual = seq("sutruaua", 3);
        let expected = "s[utru]aua";
        assert_eq!(actual, expected);

        // Special cases
        let actual = seq("saua", 0);
        let expected = "s[aua]";
        assert_eq!(actual, expected);

        let actual = seq("suaut", 0);
        let expected = "s[uau]t";
        assert_eq!(actual, expected);

        // Edge cases
        let actual = seq("", 0);
        let expected = "";
        assert_eq!(actual, expected);

        let actual = seq("s", 0);
        let expected = "s";
        assert_eq!(actual, expected);

        let actual = seq("sua", 3);
        let expected = "sua";
        assert_eq!(actual, expected);

        let actual = seq("ut", 0);
        let expected = "[u]t";
        assert_eq!(actual, expected);

        let actual = seq("suuu", 0);
        let expected = "s[uuu]";
        assert_eq!(actual, expected);

        let actual = seq("ut", 1);
        let expected = "ut";
        assert_eq!(actual, expected);

        let actual = seq("ua", 0);
        let expected = "[ua]";
        assert_eq!(actual, expected);
    }
}
