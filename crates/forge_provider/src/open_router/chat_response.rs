use std::collections::HashMap;

use forge_tool::ToolName;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::model::{Response as ModelResponse, ToolUsePart};
use crate::{FinishReason, UseId};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatResponse {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub created: u64,
    pub object: String,
    pub system_fingerprint: Option<String>,
    pub usage: Option<ResponseUsage>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResponseUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Choice {
    NonChat {
        finish_reason: Option<String>,
        text: String,
        error: Option<ErrorResponse>,
    },
    NonStreaming {
        logprobs: Option<serde_json::Value>,
        index: u32,
        finish_reason: Option<String>,
        message: ResponseMessage,
        error: Option<ErrorResponse>,
    },
    Streaming {
        finish_reason: Option<String>,
        delta: ResponseMessage,
        error: Option<ErrorResponse>,
    },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ErrorResponse {
    pub code: u32,
    pub message: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResponseMessage {
    pub content: Option<String>,
    pub role: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub refusal: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolCall {
    pub id: Option<UseId>,
    pub r#type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FunctionCall {
    pub name: Option<ToolName>,
    pub arguments: String,
}

impl TryFrom<ChatResponse> for ModelResponse {
    type Error = Error;

    fn try_from(res: ChatResponse) -> Result<Self, Self::Error> {
        if let Some(choice) = res.choices.first() {
            let response = match choice {
                Choice::NonChat { text, finish_reason, .. } => ModelResponse::new(text.clone())
                    .finish_reason_opt(finish_reason.clone().and_then(FinishReason::parse)),
                Choice::NonStreaming { message, finish_reason, .. } => {
                    let mut resp = ModelResponse::new(message.content.clone().unwrap_or_default())
                        .finish_reason_opt(finish_reason.clone().and_then(FinishReason::parse));
                    if let Some(tool_calls) = &message.tool_calls {
                        for tool_call in tool_calls {
                            resp = resp.add_call(ToolUsePart {
                                use_id: tool_call.id.clone(),
                                name: tool_call.function.name.clone(),
                                argument_part: serde_json::from_str(&tool_call.function.arguments)?,
                            });
                        }
                    }
                    resp
                }
                Choice::Streaming { delta, finish_reason, .. } => {
                    let mut resp = ModelResponse::new(delta.content.clone().unwrap_or_default())
                        .finish_reason_opt(finish_reason.clone().and_then(FinishReason::parse));
                    if let Some(tool_calls) = &delta.tool_calls {
                        for tool_call in tool_calls {
                            resp = resp.add_call(ToolUsePart {
                                use_id: tool_call.id.clone(),
                                name: tool_call.function.name.clone(),
                                argument_part: tool_call.function.arguments.clone(),
                            });
                        }
                    }
                    resp
                }
            };

            Ok(response)
        } else {
            Err(Error::empty_response("Open Router"))
        }
    }
}
