use std::collections::HashMap;

use forge_tool::Tool;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest_middleware::reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde::{Deserialize, Serialize};

use super::error::Result;
use super::provider::{InnerProvider, Provider};
use crate::log::LoggingMiddleware;
use crate::model::{AnyMessage, Assistant, Role, System, User};
use crate::ResultStream;

const DEFAULT_MODEL: &str = "llama3";

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
struct Model {
    id: String,
    name: String,
    created: u64,
    description: String,
    context_length: u64,
    architecture: Architecture,
    pricing: Pricing,
    top_provider: TopProvider,
    per_request_limits: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
struct Architecture {
    modality: String,
    tokenizer: String,
    instruct_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
struct Pricing {
    prompt: String,
    completion: String,
    image: String,
    request: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
struct TopProvider {
    context_length: Option<u64>,
    max_completion_tokens: Option<u64>,
    is_moderated: bool,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
struct ListModelResponse {
    data: Vec<Model>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct Request {
    #[serde(skip_serializing_if = "Option::is_none")]
    messages: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repetition_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    logit_bias: Option<HashMap<u32, f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_logprobs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_a: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prediction: Option<Prediction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transforms: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    models: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    route: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<ProviderPreferences>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TextContent {
    r#type: String,
    text: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ImageContentPart {
    r#type: String,
    image_url: ImageUrl,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ImageUrl {
    url: String,
    detail: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
enum ContentPart {
    Text(TextContent),
    Image(ImageContentPart),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct FunctionDescription {
    description: Option<String>,
    name: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct OllamaTool {
    r#type: String,
    function: FunctionDescription,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
enum ToolChoice {
    None,
    Auto,
    Function { name: String },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ResponseFormat {
    r#type: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Prediction {
    r#type: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub model: String,
    pub created_at: String,
    pub message: Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_reason: Option<String>,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_duration: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Message {
    pub role: String,
    pub content: String,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
struct ResponseUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
enum Choice {
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
struct ErrorResponse {
    code: u32,
    message: String,
    metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ResponseMessage {
    content: Option<String>,
    role: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
    refusal: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ToolCall {
    id: Option<String>,
    r#type: String,
    function: FunctionCall,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct FunctionCall {
    name: String,
    arguments: String,
}

impl From<Tool> for OllamaTool {
    fn from(value: Tool) -> Self {
        OllamaTool {
            r#type: "function".to_string(),
            function: FunctionDescription {
                description: Some(value.description),
                name: value.id.into_string(),
                parameters: serde_json::to_value(value.input_schema).unwrap(),
            },
        }
    }
}

impl From<AnyMessage> for Message {
    fn from(value: AnyMessage) -> Self {
        match value {
            AnyMessage::Assistant(assistant) => {
                Message { role: Assistant::name(), content: assistant.content }
            }
            AnyMessage::System(sys) => Message { role: System::name(), content: sys.content },
            AnyMessage::User(usr) => Message { role: User::name(), content: usr.content },
        }
    }
}

impl From<crate::model::Request> for Request {
    fn from(value: crate::model::Request) -> Self {
        Request {
            messages: {
                let result = value
                    .tool_result
                    .into_iter()
                    .map(|tool_result| {
                        let id = tool_result.tool_use_id.0;
                        let value = tool_result.content;

                        let mut content = HashMap::new();
                        content.insert("content", value.to_string());
                        content.insert("role", "tool".to_string());
                        content.insert("tool_use_id", id);
                        Message {
                            role: User::name(),
                            content: serde_json::to_string(&content).unwrap(),
                        }
                    })
                    .collect::<Vec<_>>();

                let mut messages = value
                    .context
                    .into_iter()
                    .map(Message::from)
                    .collect::<Vec<_>>();

                messages.extend(result);

                Some(messages)
            },
            tools: {
                let tools = value
                    .tools
                    .into_iter()
                    .map(OllamaTool::from)
                    .collect::<Vec<_>>();
                if tools.is_empty() {
                    None
                } else {
                    Some(tools)
                }
            },
            ..Default::default()
        }
    }
}

impl TryFrom<Response> for crate::model::Response {
    type Error = crate::error::Error;

    fn try_from(res: Response) -> Result<Self> {
        let message = crate::model::Message::assistant(res.message.content);

        Ok(crate::model::Response { message, tool_use: vec![] })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ProviderPreferences {
    // Define fields as necessary
}

#[derive(Clone)]
struct OllamaProvider {
    http_client: ClientWithMiddleware,

    base_url: String,
    model: String,
}

impl OllamaProvider {
    fn new(model: Option<String>, base_url: Option<String>) -> Self {
        let reqwest_client = Client::builder().build().unwrap();
        let http_client = ClientBuilder::new(reqwest_client)
            .with(LoggingMiddleware)
            .build();

        Self {
            http_client,

            base_url: base_url.unwrap_or("http://localhost:11434".to_string()),
            model: model.unwrap_or(DEFAULT_MODEL.to_string()),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

#[async_trait::async_trait]
impl InnerProvider for OllamaProvider {
    async fn chat(
        &self,
        request: crate::model::Request,
    ) -> Result<ResultStream<crate::model::Response>> {
        let mut new_request = Request::from(request);

        new_request.model = self.model.clone();
        new_request.stream = Some(true); // Ensure streaming is enabled

        let body = serde_json::to_string(&new_request)?;

        tracing::debug!("Ollama Request Body: {}", body);

        let response_stream = self
            .http_client
            .post(self.url("/api/chat"))
            .body(body)
            .send()
            .await?
            .bytes_stream();

        let processed_stream: BoxStream<_> = response_stream
            .map(|chunk| {
                chunk.map_err(crate::error::Error::from).and_then(|bytes| {
                    let response = serde_json::from_slice::<Response>(&bytes)
                        .map_err(crate::error::Error::from);
                    match response {
                        Ok(response) => Ok(crate::model::Response::try_from(response)?),
                        Err(err) => Err(err),
                    }
                })
            })
            .boxed();

        Ok(Box::pin(Box::new(processed_stream)))
    }

    async fn models(&self) -> Result<Vec<String>> {
        let text = self
            .http_client
            .get(self.url("/models"))
            .send()
            .await?
            .text()
            .await?;

        let response: ListModelResponse = serde_json::from_str(&text)?;

        Ok(response
            .data
            .iter()
            .map(|r| r.name.clone())
            .collect::<Vec<String>>())
    }
}

impl Provider {
    pub fn ollama(model: Option<String>, base_url: Option<String>) -> Self {
        Provider::new(OllamaProvider::new(model, base_url))
    }
}

#[cfg(test)]
mod test {
    use tokio_stream::StreamExt;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn models() -> &'static str {
        include_str!("models.json")
    }

    #[test]
    fn test_de_ser_of_models() {
        let _: ListModelResponse = serde_json::from_str(models()).unwrap();
    }

    #[test]
    fn test_de_ser_of_response() {
        let response = r#"{
            "id": "ollama-12345",
            "provider": "Ollama",
            "model": "ollama/gpt-4-stream",
            "object": "chat.completion",
            "created": 1700000000,
            "choices": [{
                "delta": {
                    "content": "Hello! How can I assist you today?"
                },
                "finish_reason": "end_turn",
                "index": 0,
                "error": null
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        }"#;

        let _: Response = serde_json::from_str(response).unwrap();
    }

    #[tokio::test]
    async fn test_chat() {
        // Start a Wiremock server
        let mock_server = MockServer::start().await;

        // Define the streaming response chunks
        let streaming_chunks = [
            r#"{"model":"llama3","created_at":"2024-12-24T03:24:43.041107573Z","message":{"role":"assistant","content":"Alo!"},"done":false}"#,
        ];

        // Create a streaming response template
        let response_template =
            ResponseTemplate::new(200).set_body_bytes(streaming_chunks[0].to_string().as_bytes());

        // Mock the streaming response for the `/api/chat` endpoint
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(response_template)
            .mount(&mock_server)
            .await;

        // Use the Wiremock server's URL as the base URL for the provider
        let base_url = mock_server.uri();

        // Create the actual HTTP client with middleware
        let reqwest_client = Client::builder().build().unwrap();
        let http_client = ClientBuilder::new(reqwest_client).build();

        let provider = OllamaProvider {
            http_client,
            base_url: base_url.clone(),
            model: DEFAULT_MODEL.to_string(),
        };

        // Make the chat request and handle the streaming response
        let result_stream = provider
            .chat(crate::model::Request {
                context: vec![
                    AnyMessage::System(crate::model::Message {
                        content: "If someone says Hello!, always Reply with single word Alo!"
                            .to_string(),
                        role: System,
                    }),
                    AnyMessage::User(crate::model::Message {
                        role: User,
                        content: "Hello!".to_string(),
                    }),
                ],
                tools: vec![],
                tool_result: vec![],
            })
            .await
            .unwrap();

        let messages = result_stream.collect::<Vec<_>>().await;
        let message = messages
            .into_iter()
            .filter_map(|v| v.ok())
            .map(|v| v.message.content.trim().to_string())
            .collect::<Vec<_>>()
            .join("");

        assert_eq!(message, "Alo!");
    }
}
