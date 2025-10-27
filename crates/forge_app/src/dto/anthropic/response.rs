use forge_domain::{
    ChatCompletionMessage, Content, ModelId, Reasoning, ReasoningPart, TokenCount, ToolCallId,
    ToolCallPart, ToolName,
};
use serde::Deserialize;

use super::request::Role;
use crate::dto::anthropic::Error;

#[derive(Deserialize)]
pub struct ListModelResponse {
    pub data: Vec<Model>,
}

#[derive(Deserialize)]
pub struct Model {
    pub id: String,
    pub display_name: String,
}

impl From<Model> for forge_domain::Model {
    fn from(value: Model) -> Self {
        let context_length = get_context_length(&value.id);
        Self {
            id: ModelId::new(value.id),
            name: Some(value.display_name),
            description: None,
            context_length,
            tools_supported: Some(true),
            supports_parallel_tool_calls: None,
            supports_reasoning: None,
        }
    }
}

/// Returns the context window size for a given Claude model ID.
///
/// Context lengths are based on official Claude documentation:
/// <https://docs.claude.com/en/docs/about-claude/models/overview>
///
/// # Arguments
///
/// * `model_id` - The Claude model identifier (e.g.,
///   "claude-sonnet-4-5-20250929")
///
/// # Returns
///
/// Returns `Some(tokens)` for known models, `None` for unknown models.
///
/// # Notes
///
/// - Most current models support 200K tokens
/// - Claude Sonnet 4.5 supports 1M tokens with the `context-1m-2025-08-07` beta
///   header
/// - Legacy models may have different context lengths
fn get_context_length(model_id: &str) -> Option<u64> {
    // Current models (200K context)
    if model_id.starts_with("claude-sonnet-4-5-")
        || model_id.starts_with("claude-haiku-4-5-")
        || model_id.starts_with("claude-opus-4-1-")
    {
        return Some(200_000);
    }

    // Legacy Claude 4 models (200K context)
    if model_id.starts_with("claude-sonnet-4-")
        || model_id.starts_with("claude-opus-4-")
        || model_id.starts_with("claude-3-7-sonnet-")
    {
        return Some(200_000);
    }

    // Claude 3.5 models (200K context)
    if model_id.starts_with("claude-3-5-sonnet-") || model_id.starts_with("claude-3-5-haiku-") {
        return Some(200_000);
    }

    // Claude 3 Opus and Sonnet (200K context)
    if model_id.starts_with("claude-3-opus-") || model_id.starts_with("claude-3-sonnet-") {
        return Some(200_000);
    }

    // Claude 3 Haiku (200K context)
    if model_id.starts_with("claude-3-haiku-") {
        return Some(200_000);
    }

    // Claude 2.1 (200K context)
    if model_id.starts_with("claude-2.1") {
        return Some(200_000);
    }

    // Claude 2.0 (100K context)
    if model_id.starts_with("claude-2.0") {
        return Some(100_000);
    }

    // Claude Instant (100K context)
    if model_id.starts_with("claude-instant-") {
        return Some(100_000);
    }

    // Unknown model
    None
}

#[derive(Deserialize, PartialEq, Clone, Debug)]
pub struct MessageStart {
    pub id: String,
    pub r#type: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub stop_reason: Option<StopReason>,
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}

#[derive(Deserialize, PartialEq, Clone, Debug)]
pub struct Usage {
    pub input_tokens: Option<usize>,
    pub output_tokens: Option<usize>,

    pub cache_read_input_tokens: Option<usize>,
    pub cache_creation_input_tokens: Option<usize>,
}

impl From<Usage> for forge_domain::Usage {
    fn from(usage: Usage) -> Self {
        // Anthropic token breakdown:
        // - input_tokens: tokens NOT from cache (billed at full price)
        // - cache_creation_input_tokens: tokens written to cache (billed at full price
        //   + write cost)
        // - cache_read_input_tokens: tokens read from cache (billed at 90% discount)
        // Total input = input_tokens + cache_creation_input_tokens +
        // cache_read_input_tokens

        let input_tokens = usage.input_tokens.unwrap_or_default();
        let cache_creation = usage.cache_creation_input_tokens.unwrap_or_default();
        let cache_read = usage.cache_read_input_tokens.unwrap_or_default();

        // prompt_tokens should include ALL input tokens
        let prompt_tokens = TokenCount::Actual(input_tokens + cache_creation + cache_read);

        let completion_tokens = usage
            .output_tokens
            .map(TokenCount::Actual)
            .unwrap_or_default();

        // cached_tokens represents tokens that benefited from caching
        // This includes both cache creation and cache reads
        let cached_tokens = TokenCount::Actual(cache_read);

        let total_tokens = prompt_tokens + completion_tokens;

        forge_domain::Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens,
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
}

impl From<StopReason> for forge_domain::FinishReason {
    fn from(value: StopReason) -> Self {
        match value {
            StopReason::EndTurn => forge_domain::FinishReason::Stop,
            StopReason::MaxTokens => forge_domain::FinishReason::Length,
            StopReason::StopSequence => forge_domain::FinishReason::Stop,
            StopReason::ToolUse => forge_domain::FinishReason::ToolCalls,
        }
    }
}

#[derive(Deserialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Event {
    Error {
        error: Error,
    },
    MessageStart {
        message: MessageStart,
    },
    ContentBlockStart {
        index: u32,
        content_block: ContentBlock,
    },
    Ping,
    ContentBlockDelta {
        index: u32,
        delta: ContentBlock,
    },
    ContentBlockStop {
        index: u32,
    },
    MessageDelta {
        delta: MessageDelta,
        usage: Usage,
    },
    MessageStop,
}

#[derive(Deserialize, PartialEq, Clone, Debug)]
#[serde(untagged)]
pub enum EventData {
    KnownEvent(Event),
    // To handle any unknown events:
    // ref: https://docs.anthropic.com/en/api/messages-streaming#other-events
    Unknown(serde_json::Value),
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct MessageDelta {
    pub stop_reason: StopReason,
    pub stop_sequence: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    TextDelta {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    InputJsonDelta {
        partial_json: String,
    },
    Thinking {
        thinking: Option<String>,
        signature: Option<String>,
    },
    ThinkingDelta {
        thinking: Option<String>,
    },
    SignatureDelta {
        signature: Option<String>,
    },
    RedactedThinking {
        data: Option<String>,
    },
}

impl TryFrom<EventData> for ChatCompletionMessage {
    type Error = anyhow::Error;
    fn try_from(value: EventData) -> Result<Self, Self::Error> {
        match value {
            EventData::KnownEvent(event) => ChatCompletionMessage::try_from(event),
            EventData::Unknown(_) => {
                // Ignore any unknown events
                Ok(ChatCompletionMessage::assistant(Content::part("")))
            }
        }
    }
}

impl TryFrom<Event> for ChatCompletionMessage {
    type Error = anyhow::Error;
    fn try_from(value: Event) -> Result<Self, Self::Error> {
        let result = match value {
            Event::ContentBlockStart { content_block, .. }
            | Event::ContentBlockDelta { delta: content_block, .. } => {
                ChatCompletionMessage::try_from(content_block)?
            }
            Event::MessageDelta { delta, usage } => {
                ChatCompletionMessage::assistant(Content::part(""))
                    .finish_reason(delta.stop_reason)
                    .usage(usage)
            }
            Event::Error { error } => {
                return Err(error.into());
            }
            _ => ChatCompletionMessage::assistant(Content::part("")),
        };

        Ok(result)
    }
}

impl TryFrom<ContentBlock> for ChatCompletionMessage {
    type Error = anyhow::Error;
    fn try_from(value: ContentBlock) -> Result<Self, Self::Error> {
        let result = match value {
            ContentBlock::Text { text } | ContentBlock::TextDelta { text } => {
                ChatCompletionMessage::assistant(Content::part(text))
            }
            ContentBlock::Thinking { thinking, signature } => {
                if let Some(thinking) = thinking {
                    ChatCompletionMessage::assistant(Content::part(""))
                        .reasoning(Content::part(thinking.clone()))
                        .add_reasoning_detail(Reasoning::Part(vec![ReasoningPart {
                            signature,
                            text: Some(thinking),
                        }]))
                } else {
                    ChatCompletionMessage::assistant(Content::part(""))
                }
            }
            ContentBlock::RedactedThinking { data } => {
                if let Some(data) = data {
                    ChatCompletionMessage::assistant(Content::part(""))
                        .reasoning(Content::part(data.clone()))
                        .add_reasoning_detail(Reasoning::Part(vec![ReasoningPart {
                            signature: None,
                            text: Some(data),
                        }]))
                } else {
                    ChatCompletionMessage::assistant(Content::part(""))
                }
            }
            ContentBlock::ThinkingDelta { thinking } => {
                if let Some(thinking) = thinking {
                    ChatCompletionMessage::assistant(Content::part(""))
                        .reasoning(Content::part(thinking.clone()))
                        .add_reasoning_detail(Reasoning::Part(vec![ReasoningPart {
                            signature: None,
                            text: Some(thinking),
                        }]))
                } else {
                    ChatCompletionMessage::assistant(Content::part(""))
                }
            }
            ContentBlock::SignatureDelta { signature } => {
                ChatCompletionMessage::assistant(Content::part("")).add_reasoning_detail(
                    Reasoning::Part(vec![ReasoningPart { signature, text: None }]),
                )
            }
            ContentBlock::ToolUse { id, name, input } => {
                // note: We've to check if the input is empty or null. else we end up adding
                // empty object `{}` as prefix to tool args.
                let is_empty =
                    input.is_null() || input.as_object().is_some_and(|map| map.is_empty());
                ChatCompletionMessage::assistant(Content::part("")).add_tool_call(ToolCallPart {
                    call_id: Some(ToolCallId::new(id)),
                    name: Some(ToolName::new(name)),
                    arguments_part: if is_empty {
                        "".to_string()
                    } else {
                        serde_json::to_string(&input)?
                    },
                })
            }
            ContentBlock::InputJsonDelta { partial_json } => {
                ChatCompletionMessage::assistant(Content::part("")).add_tool_call(ToolCallPart {
                    call_id: None,
                    name: None,
                    arguments_part: partial_json,
                })
            }
        };

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unknow_event() {
        let event = r#"{"type": "random_error", "error": {"type": "overloaded_error", "message": "Overloaded"}}"#;
        let event_data = serde_json::from_str::<EventData>(event).unwrap();
        assert!(matches!(event_data, EventData::Unknown(_)));
    }

    #[test]
    fn test_event_deser() {
        let tests = vec![
            (
                "error",
                r#"{"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}"#,
                Event::Error {
                    error: Error::OverloadedError { message: "Overloaded".to_string() },
                },
            ),
            (
                "message_start",
                r#"{"type":"message_start","message":{"id":"msg_019LBLYFJ7fG3fuAqzuRQbyi","type":"message","role":"assistant","content":[],"model":"claude-3-opus-20240229","stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":1}}}"#,
                Event::MessageStart {
                    message: MessageStart {
                        id: "msg_019LBLYFJ7fG3fuAqzuRQbyi".to_string(),
                        r#type: "message".to_string(),
                        role: Role::Assistant,
                        content: vec![],
                        model: "claude-3-opus-20240229".to_string(),
                        stop_reason: None,
                        stop_sequence: None,
                        usage: Usage {
                            input_tokens: Some(10),
                            output_tokens: Some(1),
                            cache_creation_input_tokens: None,
                            cache_read_input_tokens: None,
                        },
                    },
                },
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
                Event::ContentBlockStart {
                    index: 0,
                    content_block: ContentBlock::Text { text: "".to_string() },
                },
            ),
            ("ping", r#"{"type": "ping"}"#, Event::Ping),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
                Event::ContentBlockDelta {
                    index: 0,
                    delta: ContentBlock::TextDelta { text: "Hello".to_string() },
                },
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}"#,
                Event::ContentBlockDelta {
                    index: 0,
                    delta: ContentBlock::TextDelta { text: "!".to_string() },
                },
            ),
            (
                "content_block_stop",
                r#"{"type":"content_block_stop","index":0}"#,
                Event::ContentBlockStop { index: 0 },
            ),
            (
                "message_delta",
                r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":12}}"#,
                Event::MessageDelta {
                    delta: MessageDelta { stop_reason: StopReason::EndTurn, stop_sequence: None },
                    usage: Usage {
                        input_tokens: None,
                        output_tokens: Some(12),
                        cache_creation_input_tokens: None,
                        cache_read_input_tokens: None,
                    },
                },
            ),
            (
                "message_stop",
                r#"{"type":"message_stop"}"#,
                Event::MessageStop,
            ),
        ];
        for (name, input, expected) in tests {
            let actual: Event = serde_json::from_str(input).unwrap();
            assert_eq!(actual, expected, "test failed for event data: {name}");
        }
    }

    #[test]
    fn test_model_deser() {
        let input = r#"{
            "data": [
                {
                    "type": "model",
                    "id": "claude-3-5-sonnet-20241022",
                    "display_name": "Claude 3.5 Sonnet (New)",
                    "created_at": "2024-10-22T00:00:00Z"
                },
                {
                    "type": "model",
                    "id": "claude-3-5-haiku-20241022",
                    "display_name": "Claude 3.5 Haiku",
                    "created_at": "2024-10-22T00:00:00Z"
                }
            ],
            "has_more": false,
            "first_id": "claude-3-5-sonnet-20241022",
            "last_id": "claude-3-opus-20240229"
        }"#;
        let response = serde_json::from_str::<ListModelResponse>(input);
        assert!(response.is_ok());
        assert!(response.unwrap().data.len() == 2);
    }

    #[test]
    fn test_usage_conversion_with_cache_read_tokens() {
        use forge_domain::TokenCount;

        // Simulate a response with cache reads
        let fixture = Usage {
            input_tokens: Some(100),
            output_tokens: Some(50),
            cache_creation_input_tokens: Some(200),
            cache_read_input_tokens: Some(300),
        };

        let actual: forge_domain::Usage = fixture.into();

        // prompt_tokens should include ALL input tokens
        let expected_prompt = TokenCount::Actual(100 + 200 + 300);
        assert_eq!(actual.prompt_tokens, expected_prompt);

        // cached_tokens should only include cache reads (tokens that benefited from
        // caching)
        let expected_cached = TokenCount::Actual(300);
        assert_eq!(actual.cached_tokens, expected_cached);

        // completion_tokens should be output tokens
        let expected_completion = TokenCount::Actual(50);
        assert_eq!(actual.completion_tokens, expected_completion);

        // total_tokens should be prompt + completion
        let expected_total = TokenCount::Actual(600 + 50);
        assert_eq!(actual.total_tokens, expected_total);
    }

    #[test]
    fn test_usage_conversion_without_cache() {
        use forge_domain::TokenCount;

        let fixture = Usage {
            input_tokens: Some(100),
            output_tokens: Some(50),
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        };

        let actual: forge_domain::Usage = fixture.into();

        let expected_prompt = TokenCount::Actual(100);
        assert_eq!(actual.prompt_tokens, expected_prompt);

        let expected_cached = TokenCount::Actual(0);
        assert_eq!(actual.cached_tokens, expected_cached);

        let expected_completion = TokenCount::Actual(50);
        assert_eq!(actual.completion_tokens, expected_completion);

        let expected_total = TokenCount::Actual(150);
        assert_eq!(actual.total_tokens, expected_total);
    }

    #[test]
    fn test_usage_conversion_cache_read_only() {
        use forge_domain::TokenCount;

        // Scenario: All tokens came from cache (cache hit)
        let fixture = Usage {
            input_tokens: Some(0),
            output_tokens: Some(50),
            cache_creation_input_tokens: Some(0),
            cache_read_input_tokens: Some(500),
        };

        let actual: forge_domain::Usage = fixture.into();

        let expected_prompt = TokenCount::Actual(500);
        assert_eq!(actual.prompt_tokens, expected_prompt);

        let expected_cached = TokenCount::Actual(500);
        assert_eq!(actual.cached_tokens, expected_cached);

        // Cache percentage should be 100%
        let cache_percentage = (*actual.cached_tokens * 100) / *actual.prompt_tokens;
        assert_eq!(cache_percentage, 100);
    }

    #[test]
    fn test_get_context_length_current_models() {
        // Current models (200K context)
        assert_eq!(
            get_context_length("claude-sonnet-4-5-20250929"),
            Some(200_000)
        );
        assert_eq!(
            get_context_length("claude-haiku-4-5-20251001"),
            Some(200_000)
        );
        assert_eq!(
            get_context_length("claude-opus-4-1-20250805"),
            Some(200_000)
        );
    }

    #[test]
    fn test_get_context_length_legacy_claude_4() {
        // Legacy Claude 4 models (200K context)
        assert_eq!(
            get_context_length("claude-sonnet-4-20250514"),
            Some(200_000)
        );
        assert_eq!(get_context_length("claude-opus-4-20250514"), Some(200_000));
        assert_eq!(
            get_context_length("claude-3-7-sonnet-20250219"),
            Some(200_000)
        );
    }

    #[test]
    fn test_get_context_length_claude_3_5() {
        // Claude 3.5 models (200K context)
        assert_eq!(
            get_context_length("claude-3-5-sonnet-20241022"),
            Some(200_000)
        );
        assert_eq!(
            get_context_length("claude-3-5-haiku-20241022"),
            Some(200_000)
        );
    }

    #[test]
    fn test_get_context_length_claude_3() {
        // Claude 3 models (200K context)
        assert_eq!(get_context_length("claude-3-opus-20240229"), Some(200_000));
        assert_eq!(
            get_context_length("claude-3-sonnet-20240229"),
            Some(200_000)
        );
        assert_eq!(get_context_length("claude-3-haiku-20240307"), Some(200_000));
    }

    #[test]
    fn test_get_context_length_claude_2() {
        // Claude 2.1 (200K context)
        assert_eq!(get_context_length("claude-2.1"), Some(200_000));

        // Claude 2.0 (100K context)
        assert_eq!(get_context_length("claude-2.0"), Some(100_000));
    }

    #[test]
    fn test_get_context_length_claude_instant() {
        // Claude Instant (100K context)
        assert_eq!(get_context_length("claude-instant-1.2"), Some(100_000));
    }

    #[test]
    fn test_get_context_length_unknown_model() {
        // Unknown models should return None
        assert_eq!(get_context_length("unknown-model"), None);
        assert_eq!(get_context_length("claude-future-5-0"), None);
        assert_eq!(get_context_length(""), None);
    }

    #[test]
    fn test_model_conversion_includes_context_length() {
        let fixture = Model {
            id: "claude-sonnet-4-5-20250929".to_string(),
            display_name: "Claude 3.5 Sonnet (New)".to_string(),
        };

        let actual: forge_domain::Model = fixture.into();

        assert_eq!(actual.context_length, Some(200_000));
        assert_eq!(actual.id.as_str(), "claude-sonnet-4-5-20250929");
        assert_eq!(actual.name, Some("Claude 3.5 Sonnet (New)".to_string()));
    }

    #[test]
    fn test_model_conversion_unknown_model_no_context() {
        let fixture = Model {
            id: "unknown-claude-model".to_string(),
            display_name: "Unknown Model".to_string(),
        };

        let actual: forge_domain::Model = fixture.into();

        assert_eq!(actual.context_length, None);
        assert_eq!(actual.id.as_str(), "unknown-claude-model");
    }
}
