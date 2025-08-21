use std::str::FromStr;

use forge_domain::{
    ChatCompletionMessage, Content, FinishReason, TokenCount, ToolCallFull, ToolCallId,
    ToolCallPart, ToolName, Usage,
};
use serde::{Deserialize, Serialize};

use super::tool_choice::FunctionType;
use crate::dto::openai::ReasoningDetail;
use crate::dto::openai::error::{Error, ErrorResponse};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Response {
    Success {
        id: String,
        provider: Option<String>,
        model: String,
        choices: Vec<Choice>,
        created: u64,
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
    #[serde(alias = "reasoning_content")]
    pub reasoning: Option<String>,
    pub role: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub refusal: Option<String>,
    pub reasoning_details: Option<Vec<ReasoningDetail>>,
}

impl From<ReasoningDetail> for forge_domain::ReasoningFull {
    fn from(detail: ReasoningDetail) -> Self {
        forge_domain::ReasoningFull { text: detail.text, signature: detail.signature }
    }
}

impl From<ReasoningDetail> for forge_domain::ReasoningPart {
    fn from(detail: ReasoningDetail) -> Self {
        forge_domain::ReasoningPart { text: detail.text, signature: detail.signature }
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
                    // Check if the choice has an error first
                    let error = match choice {
                        Choice::NonChat { error, .. } => error,
                        Choice::NonStreaming { error, .. } => error,
                        Choice::Streaming { error, .. } => error,
                    };

                    if let Some(error) = error {
                        return Err(Error::Response(error.clone()).into());
                    }

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
                                let converted_details: Vec<forge_domain::ReasoningFull> =
                                    reasoning_details
                                        .clone()
                                        .into_iter()
                                        .map(forge_domain::ReasoningFull::from)
                                        .collect();

                                resp = resp.add_reasoning_detail(forge_domain::Reasoning::Full(
                                    converted_details,
                                ));
                            }

                            if let Some(tool_calls) = &message.tool_calls {
                                for tool_call in tool_calls {
                                    resp = resp.add_tool_call(ToolCallFull {
                                        call_id: tool_call.id.clone(),
                                        name: tool_call
                                            .function
                                            .name
                                            .clone()
                                            .ok_or(forge_domain::Error::ToolCallMissingName)?,
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
                                let converted_details: Vec<forge_domain::ReasoningPart> =
                                    reasoning_details
                                        .clone()
                                        .into_iter()
                                        .map(forge_domain::ReasoningPart::from)
                                        .collect();
                                resp = resp.add_reasoning_detail(forge_domain::Reasoning::Part(
                                    converted_details,
                                ));
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
    use forge_domain::ChatCompletionMessage;

    use super::*;

    struct Fixture;

    async fn load_fixture(filename: &str) -> serde_json::Value {
        let fixture_path = format!("src/dto/openai/fixtures/{}", filename);
        let fixture_content = tokio::fs::read_to_string(&fixture_path)
            .await
            .unwrap_or_else(|_| panic!("Failed to read fixture file: {}", fixture_path));
        serde_json::from_str(&fixture_content)
            .unwrap_or_else(|_| panic!("Failed to parse JSON fixture: {}", fixture_path))
    }

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
    #[test]
    fn test_choice_error_handling_non_chat() {
        let error_response = ErrorResponse::default().message("Test error message".to_string());

        let response = Response::Success {
            id: "test-id".to_string(),
            provider: Some("test".to_string()),
            model: "test-model".to_string(),
            choices: vec![Choice::NonChat {
                text: "test content".to_string(),
                finish_reason: None,
                error: Some(error_response.clone()),
            }],
            created: 123456789,
            object: Some("chat.completion".to_string()),
            system_fingerprint: None,
            usage: None,
        };

        let result = ChatCompletionMessage::try_from(response);
        assert!(result.is_err());
        let error = result.unwrap_err();
        let error_string = format!("{:?}", error);
        assert!(error_string.contains("Test error message"));
    }

    #[test]
    fn test_choice_error_handling_non_streaming() {
        let error_response = ErrorResponse::default().message("API limit exceeded".to_string());

        let response = Response::Success {
            id: "test-id".to_string(),
            provider: Some("test".to_string()),
            model: "test-model".to_string(),
            choices: vec![Choice::NonStreaming {
                logprobs: None,
                index: 0,
                finish_reason: None,
                message: ResponseMessage {
                    content: Some("test content".to_string()),
                    reasoning: None,
                    role: Some("assistant".to_string()),
                    tool_calls: None,
                    refusal: None,
                    reasoning_details: None,
                },
                error: Some(error_response.clone()),
            }],
            created: 123456789,
            object: Some("chat.completion".to_string()),
            system_fingerprint: None,
            usage: None,
        };

        let result = ChatCompletionMessage::try_from(response);
        assert!(result.is_err());
        let error = result.unwrap_err();
        let error_string = format!("{:?}", error);
        assert!(error_string.contains("API limit exceeded"));
    }

    #[test]
    fn test_choice_error_handling_streaming() {
        let error_response = ErrorResponse::default().message("Stream interrupted".to_string());

        let response = Response::Success {
            id: "test-id".to_string(),
            provider: Some("test".to_string()),
            model: "test-model".to_string(),
            choices: vec![Choice::Streaming {
                finish_reason: None,
                delta: ResponseMessage {
                    content: Some("test content".to_string()),
                    reasoning: None,
                    role: Some("assistant".to_string()),
                    tool_calls: None,
                    refusal: None,
                    reasoning_details: None,
                },
                error: Some(error_response.clone()),
            }],
            created: 123456789,
            object: Some("chat.completion".to_string()),
            system_fingerprint: None,
            usage: None,
        };

        let result = ChatCompletionMessage::try_from(response);
        assert!(result.is_err());
        let error = result.unwrap_err();
        let error_string = format!("{:?}", error);
        assert!(error_string.contains("Stream interrupted"));
    }

    #[test]
    fn test_choice_no_error_success() {
        let response = Response::Success {
            id: "test-id".to_string(),
            provider: Some("test".to_string()),
            model: "test-model".to_string(),
            choices: vec![Choice::NonStreaming {
                logprobs: None,
                index: 0,
                finish_reason: Some("stop".to_string()),
                message: ResponseMessage {
                    content: Some("Hello, world!".to_string()),
                    reasoning: None,
                    role: Some("assistant".to_string()),
                    tool_calls: None,
                    refusal: None,
                    reasoning_details: None,
                },
                error: None,
            }],
            created: 123456789,
            object: Some("chat.completion".to_string()),
            system_fingerprint: None,
            usage: None,
        };

        let result = ChatCompletionMessage::try_from(response);
        assert!(result.is_ok());
        let message = result.unwrap();
        assert_eq!(message.content.unwrap().as_str(), "Hello, world!");
    }

    #[test]
    fn test_empty_choices_no_error() {
        let response = Response::Success {
            id: "test-id".to_string(),
            provider: Some("test".to_string()),
            model: "test-model".to_string(),
            choices: vec![],
            created: 123456789,
            object: Some("chat.completion".to_string()),
            system_fingerprint: None,
            usage: None,
        };

        let result = ChatCompletionMessage::try_from(response);
        assert!(result.is_ok());
        let message = result.unwrap();
        assert_eq!(message.content.unwrap().as_str(), "");
    }

    #[tokio::test]
    async fn test_z_ai_response_compatibility() {
        let fixture = load_fixture("zai_api_delta_response.json").await;
        let actual = serde_json::from_value::<Response>(fixture);

        assert!(actual.is_ok());

        let response = actual.unwrap();
        let completion_result = ChatCompletionMessage::try_from(response);
        assert!(completion_result.is_ok());
    }

    #[tokio::test]
    async fn test_z_ai_response_complete_with_usage() {
        let fixture = load_fixture("zai_api_response.json").await;
        let actual = serde_json::from_value::<Response>(fixture);

        assert!(actual.is_ok());

        let response = actual.unwrap();
        let completion_result = ChatCompletionMessage::try_from(response);
        assert!(completion_result.is_ok());
    }
}
