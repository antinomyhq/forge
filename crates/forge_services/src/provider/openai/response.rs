use std::str::FromStr;

use forge_app::domain::{
    ChatCompletionMessage, Content, FinishReason, TokenCount, ToolCallFull, ToolCallId,
    ToolCallPart, ToolName, Usage,
};
use serde::{Deserialize, Serialize};

use super::super::error::{Error, ErrorResponse};
use super::tool_choice::FunctionType;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Response {
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
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub cost: Option<f64>,
    pub prompt_tokens_details: Option<PromptTokenDetails>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PromptTokenDetails {
    pub cached_tokens: usize,
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
pub struct ResponseMessage {
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub role: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub refusal: Option<String>,
    pub reasoning_details: Option<Vec<ReasoningDetail>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReasoningDetail {
    pub r#type: Option<String>,
    pub text: Option<String>,
    pub signature: Option<String>,
}

impl From<ReasoningDetail> for forge_app::domain::ReasoningFull {
    fn from(detail: ReasoningDetail) -> Self {
        forge_app::domain::ReasoningFull { text: detail.text, signature: detail.signature }
    }
}

impl From<ReasoningDetail> for forge_app::domain::ReasoningPart {
    fn from(detail: ReasoningDetail) -> Self {
        forge_app::domain::ReasoningPart { text: detail.text, signature: detail.signature }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolCall {
    pub id: Option<ToolCallId>,
    pub r#type: FunctionType,
    pub function: FunctionCall,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FunctionCall {
    // Only the first event typically has the name of the function call
    pub name: Option<ToolName>,
    #[serde(default)]
    pub arguments: String,
}

impl From<ResponseUsage> for Usage {
    fn from(usage: ResponseUsage) -> Self {
        Usage {
            prompt_tokens: TokenCount::Actual(usage.prompt_tokens),
            completion_tokens: TokenCount::Actual(usage.completion_tokens),
            total_tokens: TokenCount::Actual(usage.total_tokens),
            cached_tokens: usage
                .prompt_tokens_details
                .map(|token_details| TokenCount::Actual(token_details.cached_tokens))
                .unwrap_or_default(),
            cost: usage.cost,
        }
    }
}

impl TryFrom<Response> for ChatCompletionMessage {
    type Error = anyhow::Error;

    fn try_from(res: Response) -> Result<Self, Self::Error> {
        match res {
            Response::Success { choices, usage, .. } => {
                if let Some(choice) = choices.first() {
                    let mut response = match choice {
                        Choice::NonChat { text, finish_reason, .. } => {
                            ChatCompletionMessage::assistant(Content::full(text)).finish_reason_opt(
                                finish_reason
                                    .clone()
                                    .and_then(|s| FinishReason::from_str(&s).ok()),
                            )
                        }
                        Choice::NonStreaming { message, finish_reason, .. } => {
                            let mut resp = ChatCompletionMessage::assistant(Content::full(
                                message.content.clone().unwrap_or_default(),
                            ))
                            .finish_reason_opt(
                                finish_reason
                                    .clone()
                                    .and_then(|s| FinishReason::from_str(&s).ok()),
                            );
                            if let Some(reasoning) = &message.reasoning {
                                resp = resp.reasoning(Content::full(reasoning.clone()));
                            }

                            if let Some(reasoning_details) = &message.reasoning_details {
                                let converted_details: Vec<forge_app::domain::ReasoningFull> =
                                    reasoning_details
                                        .clone()
                                        .into_iter()
                                        .map(forge_app::domain::ReasoningFull::from)
                                        .collect();

                                resp = resp.add_reasoning_detail(
                                    forge_app::domain::Reasoning::Full(converted_details),
                                );
                            }

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
                            let mut resp = ChatCompletionMessage::assistant(Content::part(
                                delta.content.clone().unwrap_or_default(),
                            ))
                            .finish_reason_opt(
                                finish_reason
                                    .clone()
                                    .and_then(|s| FinishReason::from_str(&s).ok()),
                            );

                            if let Some(reasoning) = &delta.reasoning {
                                resp = resp.reasoning(Content::part(reasoning.clone()));
                            }

                            if let Some(reasoning_details) = &delta.reasoning_details {
                                let converted_details: Vec<forge_app::domain::ReasoningPart> =
                                    reasoning_details
                                        .clone()
                                        .into_iter()
                                        .map(forge_app::domain::ReasoningPart::from)
                                        .collect();
                                resp = resp.add_reasoning_detail(
                                    forge_app::domain::Reasoning::Part(converted_details),
                                );
                            }

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
                    let default_response = ChatCompletionMessage::assistant(Content::full(""));
                    Ok(default_response)
                }
            }
            Response::Failure { error } => Err(Error::Response(error).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use forge_app::domain::ChatCompletionMessage;

    use super::*;

    struct Fixture;

    impl Fixture {
        // check if the response is compatible with the
        fn test_response_compatibility(message: &str) -> bool {
            let response = serde_json::from_str::<Response>(message)
                .with_context(|| format!("Failed to parse response: {message}"))
                .and_then(|event| {
                    ChatCompletionMessage::try_from(event.clone())
                        .with_context(|| "Failed to create completion message")
                });
            response.is_ok()
        }
    }

    #[test]
    fn test_open_ai_response_event() {
        let event = "{\"id\":\"chatcmpl-B2YVxGR9TaLBrEcFMVCv2B4IcNe4g\",\"object\":\"chat.completion.chunk\",\"created\":1739949029,\"model\":\"gpt-4o-mini-2024-07-18\",\"service_tier\":\"default\",\"system_fingerprint\":\"fp_00428b782a\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"index\":0,\"id\":\"call_fmuXMsHhKD5eM2k0CvgNed53\",\"type\":\"function\",\"function\":{\"name\":\"forge_tool_process_shell\",\"arguments\":\"\"}}],\"refusal\":null},\"logprobs\":null,\"finish_reason\":null}]}";
        assert!(Fixture::test_response_compatibility(event));
    }

    #[test]
    fn test_forge_response_event() {
        let event = "{\"id\":\"gen-1739949430-JZMcABaj4fg8oFDtRNDZ\",\"provider\":\"OpenAI\",\"model\":\"openai/gpt-4o-mini\",\"object\":\"chat.completion.chunk\",\"created\":1739949430,\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"index\":0,\"id\":\"call_bhjvz9w48ov4DSRhM15qLMmh\",\"type\":\"function\",\"function\":{\"name\":\"forge_tool_process_shell\",\"arguments\":\"\"}}],\"refusal\":null},\"logprobs\":null,\"finish_reason\":null,\"native_finish_reason\":null}],\"system_fingerprint\":\"fp_00428b782a\"}";
        assert!(Fixture::test_response_compatibility(event));
    }

    #[test]
    fn test_reasoning_response_event() {
        let event = "{\"id\":\"gen-1751626123-nYRpHzdA0thRXF0LoQi0\",\"provider\":\"Google\",\"model\":\"anthropic/claude-3.7-sonnet:thinking\",\"object\":\"chat.completion.chunk\",\"created\":1751626123,\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\",\"reasoning\":\"I need to check\",\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\"I need to check\"}]},\"finish_reason\":null,\"native_finish_reason\":null,\"logprobs\":null}]}";
        assert!(Fixture::test_response_compatibility(event));
    }

    #[test]
    fn test_fireworks_response_event_missing_arguments() {
        let event = "{\"id\":\"gen-1749331907-SttL6PXleVHnrdLMABfU\",\"provider\":\"Fireworks\",\"model\":\"qwen/qwen3-235b-a22b\",\"object\":\"chat.completion.chunk\",\"created\":1749331907,\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"index\":0,\"id\":\"call_Wl2L8rrzHwrXSeiciIvU65IS\",\"type\":\"function\",\"function\":{\"name\":\"forge_tool_attempt_completion\"}}]},\"finish_reason\":null,\"native_finish_reason\":null,\"logprobs\":null}]}";
        assert!(Fixture::test_response_compatibility(event));
    }

    #[test]
    fn test_responses() -> anyhow::Result<()> {
        let input = include_str!("./responses.jsonl").split("\n");
        for (i, line) in input.enumerate() {
            let i = i + 1;
            let _: Response = serde_json::from_str(line).with_context(|| {
                format!("Failed to parse response [responses.jsonl:{i}]: {line}")
            })?;
        }

        Ok(())
    }
}
