use derive_more::derive::From;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use strum_macros::EnumString;

use super::ToolCall;

#[derive(Default, Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub estimated_tokens: Option<u64>,
}

/// Represents a message that was received from the LLM provider
/// NOTE: Tool call messages are part of the larger Response object and not part
/// of the message.
#[derive(Default, Clone, Debug, Setters, PartialEq, Eq)]
#[setters(into, strip_option)]
pub struct ChatCompletionMessage {
    pub content: Option<Content>,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<FinishReason>,
    pub usage: Option<Usage>,
}

/// Represents partial or full content of a message
#[derive(Clone, Debug, PartialEq, Eq, From)]
pub enum Content {
    Part(ContentPart),
    Full(ContentFull),
}

impl Content {
    pub fn as_str(&self) -> &str {
        match self {
            Content::Part(part) => &part.0,
            Content::Full(full) => &full.0,
        }
    }

    pub fn part(content: impl ToString) -> Self {
        Content::Part(ContentPart(content.to_string()))
    }

    pub fn full(content: impl ToString) -> Self {
        Content::Full(ContentFull(content.to_string()))
    }

    pub fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }

    pub fn is_part(&self) -> bool {
        matches!(self, Content::Part(_))
    }
}

/// Used typically when streaming is enabled
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentPart(String);

/// Used typically when full responses are enabled (Streaming is disabled)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentFull(String);

/// The reason why the model stopped generating output.
/// Read more: https://platform.openai.com/docs/guides/function-calling#edge-cases
#[derive(Clone, Debug, Deserialize, Serialize, EnumString, PartialEq, Eq)]
pub enum FinishReason {
    /// The model stopped generating output because it reached the maximum
    /// allowed length.
    #[strum(serialize = "length")]
    Length,
    /// The model stopped generating output because it encountered content that
    /// violated filters.
    #[strum(serialize = "content_filter")]
    ContentFilter,
    /// The model stopped generating output because it made a tool call.
    #[strum(serialize = "tool_calls")]
    ToolCalls,
    /// The model stopped generating output normally.
    #[strum(serialize = "stop", serialize = "end_turn")]
    Stop,
}

impl ChatCompletionMessage {
    pub fn assistant(content: impl Into<Content>) -> ChatCompletionMessage {
        ChatCompletionMessage::default().content(content.into())
    }

    pub fn add_tool_call(mut self, call_tool: impl Into<ToolCall>) -> Self {
        self.tool_calls.push(call_tool.into());
        self
    }

    pub fn extend_calls(mut self, calls: Vec<impl Into<ToolCall>>) -> Self {
        self.tool_calls.extend(calls.into_iter().map(Into::into));
        self
    }

    pub fn finish_reason_opt(mut self, reason: Option<FinishReason>) -> Self {
        self.finish_reason = reason;
        self
    }

    pub fn content_part(mut self, content: impl ToString) -> Self {
        self.content = Some(Content::Part(ContentPart(content.to_string())));
        self
    }

    pub fn content_full(mut self, content: impl ToString) -> Self {
        self.content = Some(Content::Full(ContentFull(content.to_string())));
        self
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_finish_reason_from_str() {
        assert_eq!(
            FinishReason::from_str("length").unwrap(),
            FinishReason::Length
        );
        assert_eq!(
            FinishReason::from_str("content_filter").unwrap(),
            FinishReason::ContentFilter
        );
        assert_eq!(
            FinishReason::from_str("tool_calls").unwrap(),
            FinishReason::ToolCalls
        );
        assert_eq!(FinishReason::from_str("stop").unwrap(), FinishReason::Stop);
        assert_eq!(
            FinishReason::from_str("end_turn").unwrap(),
            FinishReason::Stop
        );
    }
}
