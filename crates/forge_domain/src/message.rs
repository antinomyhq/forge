use std::fmt;

use derive_more::derive::From;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumString, IntoStaticStr};

use super::{ToolCall, ToolCallFull};
use crate::TokenCount;
use crate::reasoning::{Reasoning, ReasoningFull};

#[derive(Default, Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    pub prompt_tokens: TokenCount,
    pub completion_tokens: TokenCount,
    pub total_tokens: TokenCount,
    pub cached_tokens: TokenCount,
    pub cost: Option<f64>,
}

impl Usage {
    /// Accumulates usage from another Usage instance
    /// Cost is summed, tokens are added using TokenCount's Add implementation
    pub fn accumulate(mut self, other: &Usage) -> Self {
        self.prompt_tokens = self.prompt_tokens + other.prompt_tokens;
        self.completion_tokens = self.completion_tokens + other.completion_tokens;
        self.total_tokens = self.total_tokens + other.total_tokens;
        self.cached_tokens = self.cached_tokens + other.cached_tokens;
        self.cost = match (self.cost, other.cost) {
            (Some(a), Some(b)) => Some(a + b),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        self
    }
}

impl fmt::Display for Usage {
    /// Formats usage as: "$0.123 1234/5678 45.67%"
    /// - Cost (if available): "$0.123 "
    /// - Token counts: "prompt_tokens/completion_tokens"
    /// - Cache percentage (if cached tokens exist): " 45.67%"
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();

        // Add cost if available
        if let Some(cost) = self.cost {
            parts.push(format!("${:.3}", cost));
        }

        // Add token counts
        parts.push(format!(
            "{}/{}",
            *self.prompt_tokens, *self.completion_tokens
        ));

        // Add cache percentage if applicable
        let input_tokens = *self.prompt_tokens;
        let cached_tokens = *self.cached_tokens;
        if input_tokens > 0 && cached_tokens > 0 {
            let percentage = (cached_tokens as f64 / input_tokens as f64) * 100.0;
            parts.push(format!("{:.2}%", percentage));
        }

        write!(f, "{}", parts.join(" "))
    }
}

/// Represents a message that was received from the LLM provider
/// NOTE: Tool call messages are part of the larger Response object and not part
/// of the message.
#[derive(Default, Clone, Debug, Setters, PartialEq)]
#[setters(into, strip_option)]
pub struct ChatCompletionMessage {
    pub content: Option<Content>,
    pub reasoning: Option<Content>,
    pub reasoning_details: Option<Vec<Reasoning>>,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<FinishReason>,
    pub usage: Option<Usage>,
}

impl From<FinishReason> for ChatCompletionMessage {
    fn from(value: FinishReason) -> Self {
        ChatCompletionMessage::default().finish_reason(value)
    }
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

impl<T: AsRef<str>> From<T> for Content {
    fn from(value: T) -> Self {
        Content::Full(ContentFull(value.as_ref().to_string()))
    }
}

/// The reason why the model stopped generating output.
/// Read more: https://platform.openai.com/docs/guides/function-calling#edge-cases
#[derive(Clone, Debug, Deserialize, Serialize, EnumString, IntoStaticStr, PartialEq, Eq)]
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

    pub fn add_reasoning_detail(mut self, detail: impl Into<Reasoning>) -> Self {
        let detail = detail.into();
        if let Some(ref mut details) = self.reasoning_details {
            details.push(detail);
        } else {
            self.reasoning_details = Some(vec![detail]);
        }
        self
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

/// Represents a complete message from the LLM provider with all content
/// collected This is typically used after processing a stream of
/// ChatCompletionMessage
#[derive(Clone, Debug, PartialEq)]
pub struct ChatCompletionMessageFull {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<ToolCallFull>,
    pub reasoning_details: Option<Vec<ReasoningFull>>,
    pub usage: Usage,
    pub finish_reason: Option<FinishReason>,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;

    use super::*;
    #[test]
    fn test_usage_accumulate_with_both_costs() {
        let fixture_usage_1 = Usage {
            prompt_tokens: TokenCount::Actual(100),
            completion_tokens: TokenCount::Actual(50),
            total_tokens: TokenCount::Actual(150),
            cached_tokens: TokenCount::Actual(20),
            cost: Some(0.01),
        };

        let fixture_usage_2 = Usage {
            prompt_tokens: TokenCount::Actual(200),
            completion_tokens: TokenCount::Actual(75),
            total_tokens: TokenCount::Actual(275),
            cached_tokens: TokenCount::Actual(30),
            cost: Some(0.02),
        };

        let actual = fixture_usage_1.accumulate(&fixture_usage_2);

        let expected = Usage {
            prompt_tokens: TokenCount::Actual(300),
            completion_tokens: TokenCount::Actual(125),
            total_tokens: TokenCount::Actual(425),
            cached_tokens: TokenCount::Actual(50),
            cost: Some(0.03),
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_usage_accumulate_mixed_token_types() {
        let fixture_usage_1 = Usage {
            prompt_tokens: TokenCount::Actual(100),
            completion_tokens: TokenCount::Approx(50),
            total_tokens: TokenCount::Actual(150),
            cached_tokens: TokenCount::Actual(20),
            cost: Some(0.01),
        };

        let fixture_usage_2 = Usage {
            prompt_tokens: TokenCount::Approx(200),
            completion_tokens: TokenCount::Actual(75),
            total_tokens: TokenCount::Approx(275),
            cached_tokens: TokenCount::Approx(30),
            cost: Some(0.02),
        };

        let actual = fixture_usage_1.accumulate(&fixture_usage_2);

        let expected = Usage {
            prompt_tokens: TokenCount::Approx(300),
            completion_tokens: TokenCount::Approx(125),
            total_tokens: TokenCount::Approx(425),
            cached_tokens: TokenCount::Approx(50),
            cost: Some(0.03),
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_usage_accumulate_partial_costs() {
        let fixture_usage_1 = Usage {
            prompt_tokens: TokenCount::Actual(100),
            completion_tokens: TokenCount::Actual(50),
            total_tokens: TokenCount::Actual(150),
            cached_tokens: TokenCount::Actual(20),
            cost: Some(0.01),
        };

        let fixture_usage_2 = Usage {
            prompt_tokens: TokenCount::Actual(200),
            completion_tokens: TokenCount::Actual(75),
            total_tokens: TokenCount::Actual(275),
            cached_tokens: TokenCount::Actual(30),
            cost: None,
        };

        let actual = fixture_usage_1.accumulate(&fixture_usage_2);

        let expected = Usage {
            prompt_tokens: TokenCount::Actual(300),
            completion_tokens: TokenCount::Actual(125),
            total_tokens: TokenCount::Actual(425),
            cached_tokens: TokenCount::Actual(50),
            cost: Some(0.01),
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_usage_accumulate_no_costs() {
        let fixture_usage_1 = Usage {
            prompt_tokens: TokenCount::Actual(100),
            completion_tokens: TokenCount::Actual(50),
            total_tokens: TokenCount::Actual(150),
            cached_tokens: TokenCount::Actual(20),
            cost: None,
        };

        let fixture_usage_2 = Usage {
            prompt_tokens: TokenCount::Actual(200),
            completion_tokens: TokenCount::Actual(75),
            total_tokens: TokenCount::Actual(275),
            cached_tokens: TokenCount::Actual(30),
            cost: None,
        };

        let actual = fixture_usage_1.accumulate(&fixture_usage_2);

        let expected = Usage {
            prompt_tokens: TokenCount::Actual(300),
            completion_tokens: TokenCount::Actual(125),
            total_tokens: TokenCount::Actual(425),
            cached_tokens: TokenCount::Actual(50),
            cost: None,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_usage_accumulate_with_defaults() {
        let fixture_usage_1 = Usage::default();

        let fixture_usage_2 = Usage {
            prompt_tokens: TokenCount::Actual(200),
            completion_tokens: TokenCount::Actual(75),
            total_tokens: TokenCount::Actual(275),
            cached_tokens: TokenCount::Actual(30),
            cost: Some(0.05),
        };

        let actual = fixture_usage_1.accumulate(&fixture_usage_2);

        let expected = Usage {
            prompt_tokens: TokenCount::Actual(200),
            completion_tokens: TokenCount::Actual(75),
            total_tokens: TokenCount::Actual(275),
            cached_tokens: TokenCount::Actual(30),
            cost: Some(0.05),
        };

        assert_eq!(actual, expected);
    }

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

    #[test]
    fn test_usage_display_with_cost_and_cache() {
        let fixture = Usage {
            prompt_tokens: TokenCount::Actual(1000),
            completion_tokens: TokenCount::Actual(500),
            total_tokens: TokenCount::Actual(1500),
            cached_tokens: TokenCount::Actual(400),
            cost: Some(0.123),
        };

        let actual = format!("{}", fixture);
        let expected = "$0.123 1000/500 40.00%";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_usage_display_without_cost() {
        let fixture = Usage {
            prompt_tokens: TokenCount::Actual(1000),
            completion_tokens: TokenCount::Actual(500),
            total_tokens: TokenCount::Actual(1500),
            cached_tokens: TokenCount::Actual(250),
            cost: None,
        };

        let actual = format!("{}", fixture);
        let expected = "1000/500 25.00%";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_usage_display_without_cache() {
        let fixture = Usage {
            prompt_tokens: TokenCount::Actual(1000),
            completion_tokens: TokenCount::Actual(500),
            total_tokens: TokenCount::Actual(1500),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(0.045),
        };

        let actual = format!("{}", fixture);
        let expected = "$0.045 1000/500";

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_usage_display_minimal() {
        let fixture = Usage {
            prompt_tokens: TokenCount::Actual(100),
            completion_tokens: TokenCount::Actual(50),
            total_tokens: TokenCount::Actual(150),
            cached_tokens: TokenCount::Actual(0),
            cost: None,
        };

        let actual = format!("{}", fixture);
        let expected = "100/50";

        assert_eq!(actual, expected);
    }
}
