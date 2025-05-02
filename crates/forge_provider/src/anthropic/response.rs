use std::fmt::{self, Display, Formatter};

use forge_domain::{ChatCompletionMessage, Content, ModelId, ToolCallId, ToolCallPart, ToolName};
use serde::Deserialize;

use super::request::Role;

#[derive(Deserialize)]
pub struct ListModelResponse {
    pub data: Vec<Model>,
}

#[derive(Deserialize)]
pub struct Model {
    id: String,
    display_name: String,
}

impl From<Model> for forge_domain::Model {
    fn from(value: Model) -> Self {
        Self {
            id: ModelId::new(value.id),
            name: Some(value.display_name),
            description: None,
            context_length: None,
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
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl From<Usage> for forge_domain::Usage {
    fn from(usage: Usage) -> Self {
        forge_domain::Usage {
            prompt_tokens: usage.input_tokens.unwrap_or(0),
            completion_tokens: usage.output_tokens.unwrap_or(0),
            total_tokens: usage.input_tokens.unwrap_or(0) + usage.output_tokens.unwrap_or(0),
            estimated_tokens: None,
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
        error: ErrorData,
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

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ErrorData {
    OverloadedError { message: String },
}

impl Display for ErrorData {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ErrorData::OverloadedError { message } => write!(f, "OverloadedError: {message}"),
        }
    }
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
            Event::MessageDelta { delta, .. } => {
                ChatCompletionMessage::assistant(Content::part("")).finish_reason(delta.stop_reason)
            }
            Event::Error { error } => {
                return Err(anyhow::anyhow!("Anthropic API error: {}", error));
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
                    error: ErrorData::OverloadedError { message: "Overloaded".to_string() },
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
                        usage: Usage { input_tokens: Some(10), output_tokens: Some(1) },
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
                    usage: Usage { input_tokens: None, output_tokens: Some(12) },
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
