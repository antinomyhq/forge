use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;

use forge_domain::{
    ChatCompletionMessage as ModelResponse, Content, FinishReason, ToolCallFull, ToolCallId,
    ToolCallPart, ToolName, Usage,
};
use serde::{Deserialize, Serialize};

use super::error::Error;
use super::tool_choice::FunctionType;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum OpenRouterResponse {
    Success {
        id: String,
        provider: Option<String>,
        model: String,
        choices: Vec<Choice>,
        created: u64,
        object: String,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "message: {}", self.message)?;
        if !self.metadata.is_empty() {
            if let Ok(str_repr) = serde_json::to_string(&self.metadata) {
                write!(f, ", details: {str_repr}")?;
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
            estimated_tokens: None,
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
                    let default_response = ModelResponse::assistant(Content::full(""));
                    Ok(default_response)
                }
            }
            OpenRouterResponse::Failure { error } => Err(Error::Upstream(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use forge_domain::ChatCompletionMessage;

    use super::*;

    struct Fixture;

    impl Fixture {
        // check if the response is compatible with the OpenRouterResponse
        fn test_response_compatibility(message: &str) -> bool {
            let open_router_response = serde_json::from_str::<OpenRouterResponse>(message)
                .with_context(|| format!("Failed to parse OpenRouter response: {message}"))
                .and_then(|event| {
                    ChatCompletionMessage::try_from(event.clone())
                        .with_context(|| "Failed to create completion message")
                });
            open_router_response.is_ok()
        }
    }

    #[test]
    fn test_open_ai_response_event() {
        let event = "{\"id\":\"chatcmpl-B2YVxGR9TaLBrEcFMVCv2B4IcNe4g\",\"object\":\"chat.completion.chunk\",\"created\":1739949029,\"model\":\"gpt-4o-mini-2024-07-18\",\"service_tier\":\"default\",\"system_fingerprint\":\"fp_00428b782a\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"index\":0,\"id\":\"call_fmuXMsHhKD5eM2k0CvgNed53\",\"type\":\"function\",\"function\":{\"name\":\"forge_tool_process_shell\",\"arguments\":\"\"}}],\"refusal\":null},\"logprobs\":null,\"finish_reason\":null}]}";
        assert!(Fixture::test_response_compatibility(event));
    }

    #[test]
    fn test_open_router_response_event() {
        let event = "{\"id\":\"gen-1739949430-JZMcABaj4fg8oFDtRNDZ\",\"provider\":\"OpenAI\",\"model\":\"openai/gpt-4o-mini\",\"object\":\"chat.completion.chunk\",\"created\":1739949430,\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"index\":0,\"id\":\"call_bhjvz9w48ov4DSRhM15qLMmh\",\"type\":\"function\",\"function\":{\"name\":\"forge_tool_process_shell\",\"arguments\":\"\"}}],\"refusal\":null},\"logprobs\":null,\"finish_reason\":null,\"native_finish_reason\":null}],\"system_fingerprint\":\"fp_00428b782a\"}";
        assert!(Fixture::test_response_compatibility(event));
    }
}
