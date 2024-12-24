use std::collections::HashMap;

use forge_tool::{Tool, ToolId};
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue};
use reqwest_middleware::reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use super::error::Result;
use super::provider::{InnerProvider, Provider};
use crate::log::LoggingMiddleware;
use crate::model::{AnyMessage, Assistant, Role, System, ToolUse, UseId, User};
use crate::ResultStream;

const DEFAULT_MODEL: &str = "google/gemini-flash-1.5-8b-exp";

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
    tools: Option<Vec<OpenRouterTool>>,
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
    logit_bias: Option<std::collections::HashMap<u32, f32>>,
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
    // TODO: could be an enum
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
struct Message {
    role: String,
    content: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct FunctionDescription {
    description: Option<String>,
    name: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct OpenRouterTool {
    // TODO: should be an enum
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

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Response {
    id: String,
    provider: String,
    model: String,
    choices: Vec<Choice>,
    created: u64,
    object: String,
    system_fingerprint: Option<String>,
    usage: Option<ResponseUsage>,
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

impl From<Tool> for OpenRouterTool {
    fn from(value: Tool) -> Self {
        OpenRouterTool {
            r#type: "function".to_string(),
            function: FunctionDescription {
                description: Some(value.description),
                name: value.id.into_string(),
                parameters: serde_json::to_value(value.input_schema).unwrap(),
            },
        }
    }
}

// TODO: fix the names.
impl From<AnyMessage> for Message {
    fn from(value: AnyMessage) -> Self {
        match value {
            AnyMessage::Assistant(assistant) => Message {
                role: Assistant::name(),
                content: assistant.content,
                name: None,
            },
            AnyMessage::System(sys) => {
                Message { role: System::name(), content: sys.content, name: None }
            }
            AnyMessage::User(usr) => {
                Message { role: User::name(), content: usr.content, name: None }
            }
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
                        let value = tool_result.content;

                        let mut content = HashMap::new();
                        content.insert("content", value.to_string());
                        content.insert("role", "tool".to_string());
                        if let Some(id) = tool_result.tool_use_id {
                            content.insert("tool_use_id", id.0);
                        }

                        Message {
                            role: User::name(),
                            content: serde_json::to_string(&content).unwrap(),
                            name: None,
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
                    .map(OpenRouterTool::from)
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
        if let Some(choice) = res.choices.first() {
            let response = match choice {
                Choice::NonChat { text, .. } => crate::model::Response::new(text.clone()),
                Choice::NonStreaming { message, .. } => {
                    let mut resp =
                        crate::model::Response::new(message.content.clone().unwrap_or_default());
                    if let Some(tool_calls) = &message.tool_calls {
                        for tool_call in tool_calls {
                            resp = resp.add_call(ToolUse {
                                tool_use_id: tool_call.id.clone().map(UseId),
                                tool_id: ToolId::new(&tool_call.function.name),
                                input: serde_json::from_str(&tool_call.function.arguments)?,
                            });
                        }
                    }
                    resp
                }
                Choice::Streaming { delta, .. } => {
                    let mut resp =
                        crate::model::Response::new(delta.content.clone().unwrap_or_default());
                    if let Some(tool_calls) = &delta.tool_calls {
                        for tool_call in tool_calls {
                            resp = resp.add_call(ToolUse {
                                tool_use_id: tool_call.id.clone().map(UseId),
                                tool_id: ToolId::new(&tool_call.function.name),
                                input: serde_json::from_str(&tool_call.function.arguments)?,
                            });
                        }
                    }
                    resp
                }
            };
            Ok(response)
        } else {
            Err(crate::error::Error::empty_response("Open Router"))
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ProviderPreferences {
    // Define fields as necessary
}

#[derive(Debug, Clone)]
struct Config {
    api_key: String,
    base_url: Option<String>,
}

impl Config {
    fn api_base(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or("https://openrouter.ai/api/v1")
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
}

#[derive(Clone)]
struct OpenRouter {
    http_client: ClientWithMiddleware,
    config: Config,
    #[allow(unused)]
    model: String,
}

impl OpenRouter {
    fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        let config = Config { api_key, base_url };

        let reqwest_client = Client::builder().build().unwrap();
        let http_client = ClientBuilder::new(reqwest_client)
            .with(LoggingMiddleware)
            .build();

        Self {
            http_client,
            config,
            model: model.unwrap_or(DEFAULT_MODEL.to_string()),
        }
    }
}

#[async_trait::async_trait]
impl InnerProvider for OpenRouter {
    async fn chat(
        &self,
        request: crate::model::Request,
    ) -> Result<ResultStream<crate::model::Response>> {
        let mut new_request = Request::from(request);

        new_request.model = self.model.clone();

        let body = serde_json::to_string(&new_request)?;

        tracing::debug!("Request Body: {}", body);

        let response_stream = self
            .http_client
            .post(self.config.url("/chat/completions"))
            .headers(self.config.headers())
            .body(body)
            .send()
            .await?
            .bytes_stream();

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            let mut response_stream = response_stream;
            while let Some(chunk_result) = response_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        if let Ok(response) = serde_json::from_slice::<Response>(&chunk) {
                            if let Ok(model_response) = crate::model::Response::try_from(response) {
                                if tx.send(Ok(model_response)).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(Err(crate::error::Error::from(err))).await;
                        break;
                    }
                }
            }
        });

        let processed_stream = ReceiverStream::new(rx);

        Ok(Box::pin(Box::new(processed_stream)))
    }

    async fn models(&self) -> Result<Vec<String>> {
        let text = self
            .http_client
            .get(self.config.url("/models"))
            .headers(self.config.headers())
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
    pub fn open_router(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Provider::new(OpenRouter::new(api_key, model, base_url))
    }
}

#[cfg(test)]
mod test {
    use tokio_stream::StreamExt;

    use super::*;
    use crate::test_server::server::TestServer;

    fn models() -> &'static str {
        include_str!("models.json")
    }

    #[test]
    fn test_de_ser_of_models() {
        let _: ListModelResponse = serde_json::from_str(models()).unwrap();
    }

    #[test]
    fn test_de_ser_of_response() {
        let response = r#"{"id":"gen-1734752897-QSJJJjXmljCFFkUZHtFk","provider":"Anthropic","model":"anthropic/claude-3.5-sonnet","object":"chat.completion","created":1734752897,"choices":[{"logprobs":null,"finish_reason":"end_turn","index":0,"message":{"role":"assistant","content":"I aim to be direct and honest in my interactions: I'm an AI assistant, so I don't experience feelings in the way humans do. I aim to be helpful while being transparent about what I am. How can I assist you today?","refusal":""}}],"usage":{"prompt_tokens":13,"completion_tokens":54,"total_tokens":67}}"#;

        let _: Response = serde_json::from_str(response).unwrap();
    }

    #[tokio::test]
    async fn test_chat() {
        let stream = vec![
            "{\"id\":\"gen-1735016712-w3fWt8KAO98cThvdOpI7\",\"provider\":\"Google AI Studio\",\"model\":\"google/gemini-flash-1.5-8b-exp\",\"object\":\"chat.completion\",\"created\":1735016712,\"choices\":[{\"logprobs\":null,\"finish_reason\":\"STOP\",\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"A\\n\",\"refusal\":\"\"}}],\"usage\":{\"prompt_tokens\":16,\"completion_tokens\":2,\"total_tokens\":15}}".to_string(),
            "{\"id\":\"gen-1735016712-w3fWt8KAO98cThvdOpI7\",\"provider\":\"Google AI Studio\",\"model\":\"google/gemini-flash-1.5-8b-exp\",\"object\":\"chat.completion\",\"created\":1735016712,\"choices\":[{\"logprobs\":null,\"finish_reason\":\"STOP\",\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"lo!\\n\",\"refusal\":\"\"}}],\"usage\":{\"prompt_tokens\":16,\"completion_tokens\":2,\"total_tokens\":17}}".to_string(),
        ];

        let server = TestServer::new(19195, "/chat/completions", move |_req| stream.clone());
        let tx = server.start().await;

        let base_url = "http://127.0.0.1:19195".to_string();

        let mut router = OpenRouter::new("BLAH".to_string(), None, None);
        router.config.base_url = Some(base_url);

        let provider = Provider::new(router);

        let result_stream = provider
            .chat(crate::model::Request {
                context: vec![
                    AnyMessage::User(crate::model::Message {
                        role: User,
                        content: "Hello!".to_string(),
                    }),
                    AnyMessage::System(crate::model::Message {
                        role: System,
                        content: "If someone says Hello!, always Reply with single word Alo!"
                            .to_string(),
                    }),
                ],
                tools: vec![],
                tool_result: vec![],
            })
            .await
            .unwrap();

        let stream = result_stream.collect::<Vec<_>>().await;
        let resp = stream
            .into_iter()
            .filter_map(Result::ok)
            .map(|v| v.message.content.trim().trim().to_string())
            .collect::<Vec<_>>()
            .join("");

        assert_eq!(resp, "Alo!");
        let _ = tx.send(());
    }
}
