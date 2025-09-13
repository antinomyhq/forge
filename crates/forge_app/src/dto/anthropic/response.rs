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
        Self {
            id: ModelId::new(value.id),
            name: Some(value.display_name),
            description: None,
            context_length: None,
            tools_supported: Some(true),
            supports_parallel_tool_calls: None,
            supports_reasoning: None,
        }
    }
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
        let prompt_tokens = usage
            .input_tokens
            .map(TokenCount::Actual)
            .unwrap_or_default();
        let completion_tokens = usage
            .output_tokens
            .map(TokenCount::Actual)
            .unwrap_or_default();
        let cached_tokens = usage
            .cache_creation_input_tokens
            .map(TokenCount::Actual)
            .unwrap_or_default();
        let total_tokens = prompt_tokens.clone() + completion_tokens.clone();

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
}
