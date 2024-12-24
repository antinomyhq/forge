use std::pin::Pin;

use async_openai::error::OpenAIError;
use async_openai::types::*;
use async_openai::{config, Client};
use forge_tool::{Tool, ToolId};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::StreamExt;

use super::error::Result;
use super::provider::{InnerProvider, Provider};
use crate::model::{Message, Request, Response, ToolUse};
use crate::{Error, ProviderError, ResultStream};

const PROVIDER_NAME: &str = "openai";

#[derive(Debug, Clone)]
struct Config {
    api_key: String,
    base_url: Option<String>,
}

impl config::Config for Config {
    fn api_key(&self) -> &str {
        &self.api_key
    }

    fn api_base(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1")
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key)).unwrap(),
        );
        headers.insert("X-Title", HeaderValue::from_static("Tailcall"));
        headers
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base(), path)
    }

    fn query(&self) -> Vec<(&str, &str)> {
        Vec::new()
    }
}

#[derive(Clone)]
pub struct OpenRouter {
    client: Client<Config>,
    model: String,
    http_client: reqwest::Client,
    config: Config,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub created: u64,
    pub description: String,
    pub context_length: u64,
    pub architecture: Architecture,
    pub pricing: Pricing,
    pub top_provider: TopProvider,
    pub per_request_limits: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Architecture {
    pub modality: String,
    pub tokenizer: String,
    pub instruct_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Pricing {
    pub prompt: String,
    pub completion: String,
    pub image: String,
    pub request: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct TopProvider {
    pub context_length: Option<u64>,
    pub max_completion_tokens: Option<u64>,
    pub is_moderated: bool,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct ListModelResponse {
    pub object: Option<String>,
    pub data: Vec<Model>,
}

// Making new_message public
pub fn new_message(role: Role, input: &str) -> Result<ChatCompletionRequestMessage> {
    Ok(ChatCompletionRequestMessageArgs::default()
        .role(role)
        .content(input)
        .build()?)
}

// Making Role public
pub use async_openai::types::Role;

impl OpenRouter {
    fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        let config = Config { api_key, base_url };
        let http_client = reqwest::Client::new();
        let client = Client::with_config(config.clone()).with_http_client(http_client.clone());

        Self {
            client,
            http_client,
            config,
            model: model.unwrap_or("gpt-4o-mini".to_string()),
        }
    }
}

impl TryFrom<FunctionCallStream> for ToolUse {
    type Error = Error;
    fn try_from(value: FunctionCallStream) -> Result<Self> {
        Ok(ToolUse {
            tool_use_id: None,
            tool_id: ToolId::new(&value.name.ok_or(Error::Provider {
                provider: PROVIDER_NAME.to_string(),
                error: ProviderError::ToolUseEmptyName,
            })?),
            input: match value.arguments {
                Some(args) => Some(serde_json::from_str(&args)?),
                None => None,
            },
        })
    }
}

fn get_message(
    mut response: std::result::Result<CreateChatCompletionStreamResponse, OpenAIError>,
) -> Result<Response> {
    match response {
        Ok(mut response) => {
            if let Some(choice) = response.choices.pop() {
                if let Some(content) = choice.delta.content {
                    Ok(Response {
                        message: Message::assistant(content),
                        tool_use: match choice.delta.function_call {
                            Some(call) => vec![call.try_into()?],
                            None => vec![],
                        },
                    })
                } else {
                    Err(Error::Provider {
                        provider: PROVIDER_NAME.to_string(),
                        error: ProviderError::EmptyContent,
                    })
                }
            } else {
                Err(Error::Provider {
                    provider: PROVIDER_NAME.to_string(),
                    error: ProviderError::EmptyContent,
                })
            }
        }
        Err(err) => Err(err.into()),
    }
}

#[async_trait::async_trait]
impl InnerProvider for OpenRouter {
    async fn chat(&self, request: Request) -> Result<ResultStream<Response>> {
        let client = self.client.clone();
        let chat = client.chat();
        let response = chat.create_stream(request.into()).await?;

        Ok(Box::pin(response.map(get_message)))
    }

    async fn models(&self) -> Result<Vec<String>> {
        Ok(self
            .client
            .models()
            .list()
            .await?
            .data
            .iter()
            .map(|r| r.id.clone())
            .collect())
    }
}

impl Provider {
    pub fn open_ai(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Provider::new(OpenRouter::new(api_key, model, base_url))
    }
}

impl From<Request> for CreateChatCompletionRequest {
    fn from(value: Request) -> Self {
        todo!()
    }
}

impl From<CreateChatCompletionResponse> for Response {
    fn from(value: CreateChatCompletionResponse) -> Self {
        todo!()
    }
}
