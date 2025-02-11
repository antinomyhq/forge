use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;

use forge_domain::{
    ChatCompletionMessage as ModelResponse, Content, FinishReason, ToolCallFull, ToolCallId,
    ToolCallPart, ToolName, Usage,
};
use serde::{Deserialize, Serialize};

use super::tool_choice::FunctionType;
use crate::error::Error;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum OpenRouterResponse {
    Success {
        id: Option<String>,
        provider: Option<String>,
        model: Option<String>,
        choices: Vec<Choice>,
        created: Option<u64>,
        object: Option<String>,
        system_fingerprint: Option<String>,
        usage: Option<ResponseUsage>,
    },
    Failure {
        error: ErrorResponse,
    },
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
        index: Option<u32>,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "message: {}", self.message)?;
        if !self.metadata.is_empty() {
            if let Ok(str_repr) = serde_json::to_string(&self.metadata) {
                write!(f, ", details: {}", str_repr)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResponseMessage {
    pub content: Option<String>,
    pub role: Option<String>,
    pub tool_calls: Option<Vec<OpenRouterToolCall>>,
    pub refusal: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenRouterToolCall {
    pub id: Option<ToolCallId>,
    pub r#type: FunctionType,
    pub function: FunctionCall,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FunctionCall {
    // Only the first event typically has the name of the function call
    pub name: Option<ToolName>,
    pub arguments: String,
}

impl From<ResponseUsage> for Usage {
    fn from(usage: ResponseUsage) -> Self {
        Usage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        }
    }
}

impl TryFrom<OpenRouterResponse> for ModelResponse {
    type Error = Error;

    fn try_from(res: OpenRouterResponse) -> Result<Self, Self::Error> {
        match res {
            OpenRouterResponse::Success { choices, usage, .. } => {
                if let Some(choice) = choices.first() {
                    let mut response = match choice {
                        Choice::NonChat { text, finish_reason, .. } => {
                            ModelResponse::assistant(Content::full(text)).finish_reason_opt(
                                finish_reason
                                    .clone()
                                    .and_then(|s| FinishReason::from_str(&s).ok()),
                            )
                        }
                        Choice::NonStreaming { message, finish_reason, .. } => {
                            let mut resp = ModelResponse::assistant(Content::full(
                                message.content.clone().unwrap_or_default(),
                            ))
                            .finish_reason_opt(
                                finish_reason
                                    .clone()
                                    .and_then(|s| FinishReason::from_str(&s).ok()),
                            );
                            if let Some(tool_calls) = &message.tool_calls {
                                for tool_call in tool_calls {
                                    resp = resp.add_tool_call(ToolCallFull {
                                        call_id: tool_call.id.clone(),
                                        name: tool_call
                                            .function
                                            .name
                                            .clone()
                                            .ok_or(Error::ToolCallMissingName)?,
                                        arguments: serde_json::from_str(
                                            &tool_call.function.arguments,
                                        )?,
                                    });
                                }
                            }
                            resp
                        }
                        Choice::Streaming { delta, finish_reason, .. } => {
                            let mut resp = ModelResponse::assistant(Content::part(
                                delta.content.clone().unwrap_or_default(),
                            ))
                            .finish_reason_opt(
                                finish_reason
                                    .clone()
                                    .and_then(|s| FinishReason::from_str(&s).ok()),
                            );
                            if let Some(tool_calls) = &delta.tool_calls {
                                for tool_call in tool_calls {
                                    resp = resp.add_tool_call(ToolCallPart {
                                        call_id: tool_call.id.clone(),
                                        name: tool_call.function.name.clone(),
                                        arguments_part: tool_call.function.arguments.clone(),
                                    });
                                }
                            }
                            resp
                        }
                    };

                    if let Some(usage) = usage {
                        response.usage = Some(usage.into());
                    }
                    Ok(response)
                } else {
                    Err(Error::EmptyContent)
                }
            }
            OpenRouterResponse::Failure { error } => Err(Error::Upstream(error)),
        }
    }
}
