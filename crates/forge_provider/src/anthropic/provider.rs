use anyhow::Context as _;
use derive_builder::Builder;
use forge_app::domain::{
    ChatCompletionMessage, Context, Model, ModelId, ResultStream, Transformer,
};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Url};
use reqwest_eventsource::{Event, RequestBuilderExt};
use tokio_stream::StreamExt;
use tracing::debug;

use super::request::Request;
use super::response::{EventData, ListModelResponse};
use crate::anthropic::transforms::ReasoningTransform;
use crate::error::Error;
use crate::utils::format_http_context;

#[derive(Clone, Builder)]
pub struct Anthropic {
    client: Client,
    api_key: String,
    base_url: Url,
    anthropic_version: String,
}

impl Anthropic {
    pub fn builder() -> AnthropicBuilder {
        AnthropicBuilder::default()
    }

    fn url(&self, path: &str) -> anyhow::Result<Url> {
        // Validate the path doesn't contain certain patterns
        if path.contains("://") || path.contains("..") {
            anyhow::bail!("Invalid path: Contains forbidden patterns");
        }

        // Remove leading slash to avoid double slashes
        let path = path.trim_start_matches('/');

        self.base_url
            .join(path)
            .with_context(|| format!("Failed to append {} to base URL: {}", path, self.base_url))
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        // note: anthropic api requires the api key to be sent in `x-api-key` header.
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(self.api_key.as_str()).unwrap(),
        );

        // note: `anthropic-version` header is required by the API.
        headers.insert(
            "anthropic-version",
            HeaderValue::from_str(&self.anthropic_version).unwrap(),
        );
        headers
    }
}

impl Anthropic {
    pub async fn chat(
        &self,
        model: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let max_tokens = context.max_tokens.unwrap_or(4000);
        // transform the context to match the request format
        let context = ReasoningTransform.transform(context);

        let request = Request::try_from(context)?
            .model(model.as_str().to_string())
            .stream(true)
            .max_tokens(max_tokens as u64);

        let url = self.url("/messages")?;
        debug!(url = %url, model = %model, "Connecting Upstream");
        let es = self
            .client
            .post(url.clone())
            .headers(self.headers())
            .json(&request)
            .eventsource()
            .with_context(|| format_http_context(None, "POST", &url))?;

        let stream = es
            .take_while(|message| !matches!(message, Err(reqwest_eventsource::Error::StreamEnded)))
            .then(|event| async {
                match event {
                    Ok(event) => match event {
                        Event::Open => None,
                        Event::Message(event) if ["[DONE]", ""].contains(&event.data.as_str()) => {
                            debug!("Received completion from Upstream");
                            None
                        }
                        Event::Message(message) => Some(
                            serde_json::from_str::<EventData>(&message.data)
                                .with_context(|| "Failed to parse Anthropic event")
                                .and_then(|event| {
                                    ChatCompletionMessage::try_from(event).with_context(|| {
                                        format!(
                                            "Failed to create completion message: {}",
                                            message.data
                                        )
                                    })
                                }),
                        ),
                    },
                    Err(error) => match error {
                        reqwest_eventsource::Error::StreamEnded => None,
                        reqwest_eventsource::Error::InvalidStatusCode(_, response) => {
                            let status = response.status();
                            let body = response.text().await.ok();
                            Some(Err(Error::InvalidStatusCode(status.as_u16())).with_context(
                                || match body {
                                    Some(body) => {
                                        format!("Invalid status code: {status} Reason: {body}")
                                    }
                                    None => {
                                        format!("Invalid status code: {status} Reason: [Unknown]")
                                    }
                                },
                            ))
                        }
                        reqwest_eventsource::Error::InvalidContentType(_, ref response) => {
                            let status_code = response.status();
                            debug!(response = ?response, "Invalid content type");
                            Some(Err(error).with_context(|| format!("Http Status: {status_code}")))
                        }
                        error => {
                            tracing::error!(error = ?error, "Failed to receive chat completion event");
                            Some(Err(error.into()))
                        }
                    },
                }
            })
            .map(move |response| match response {
                Some(Err(err)) => {
                    Some(Err(err).with_context(|| format_http_context(None, "POST", &url)))
                }
                _ => response,
            });

        Ok(Box::pin(stream.filter_map(|x| x)))
    }

    pub async fn models(&self) -> anyhow::Result<Vec<Model>> {
        let url = self.url("models")?;
        debug!(url = %url, "Fetching models");

        let result = self
            .client
            .get(url.clone())
            .headers(self.headers())
            .send()
            .await;

        match result {
            Err(error) => {
                tracing::error!(error = ?error, "Failed to fetch models");
                let ctx_msg = format_http_context(error.status(), "GET", &url);
                Err(error)
                    .with_context(|| ctx_msg)
                    .with_context(|| "Failed to fetch models")
            }
            Ok(response) => {
                let status = response.status();
                let ctx_msg = format_http_context(Some(response.status()), "GET", &url);
                let text = response
                    .text()
                    .await
                    .with_context(|| ctx_msg.clone())
                    .with_context(|| "Failed to decode response into text")?;

                if status.is_success() {
                    let response: ListModelResponse = serde_json::from_str(&text)
                        .with_context(|| ctx_msg)
                        .with_context(|| "Failed to deserialize models response")?;
                    Ok(response.data.into_iter().map(Into::into).collect())
                } else {
                    // treat non 200 response as error.
                    Err(anyhow::anyhow!(text))
                        .with_context(|| ctx_msg)
                        .with_context(|| "Failed to fetch the models")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_app::domain::{
        Context, ContextMessage, ToolCallFull, ToolCallId, ToolChoice, ToolName, ToolOutput,
        ToolResult,
    };

    use super::*;
    use crate::mock_server::{MockServer, normalize_ports};

    fn create_anthropic(base_url: &str) -> anyhow::Result<Anthropic> {
        Ok(Anthropic::builder()
            .client(Client::new())
            .base_url(Url::parse(base_url)?)
            .anthropic_version("2023-06-01".to_string())
            .api_key("sk-test-key".to_string())
            .build()
            .unwrap())
    }

    fn create_mock_models_response() -> serde_json::Value {
        serde_json::json!({
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
        })
    }

    fn create_error_response(message: &str, code: u16) -> serde_json::Value {
        serde_json::json!({
            "error": {
                "code": code,
                "message": message
            }
        })
    }

    fn create_empty_response() -> serde_json::Value {
        serde_json::json!({
            "data": [],
        })
    }

    #[tokio::test]
    async fn test_url_for_models() {
        let anthropic = Anthropic::builder()
            .client(Client::new())
            .base_url(Url::parse("https://api.anthropic.com/v1/").unwrap())
            .anthropic_version("v1".to_string())
            .api_key("sk-some-key".to_string())
            .build()
            .unwrap();
        assert_eq!(
            anthropic.url("/models").unwrap().as_str(),
            "https://api.anthropic.com/v1/models"
        );
    }

    #[tokio::test]
    async fn test_request_conversion() {
        let model_id = ModelId::new("gpt-4");
        let context = Context::default()
            .add_message(ContextMessage::system(
                "You're expert at math, so you should resolve all user queries.",
            ))
            .add_message(ContextMessage::user(
                "what's 2 + 2 ?",
                model_id.clone().into(),
            ))
            .add_message(ContextMessage::assistant(
                "here is the system call.",
                None,
                Some(vec![ToolCallFull {
                    name: ToolName::new("math"),
                    call_id: Some(ToolCallId::new("math-1")),
                    arguments: serde_json::json!({"expression": "2 + 2"}),
                }]),
            ))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("math"),
                call_id: Some(ToolCallId::new("math-1")),
                output: ToolOutput::text(serde_json::json!({"result": 4}).to_string()),
            }])
            .tool_choice(ToolChoice::Call(ToolName::new("math")));
        let request = Request::try_from(context)
            .unwrap()
            .model("sonnet-3.5".to_string())
            .stream(true)
            .max_tokens(4000u64);
        insta::assert_snapshot!(serde_json::to_string_pretty(&request).unwrap());
    }

    #[tokio::test]
    async fn test_fetch_models_success() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_mock_models_response(), 200)
            .await;
        let anthropic = create_anthropic(&fixture.url())?;
        let actual = anthropic.models().await?;

        mock.assert_async().await;

        // Verify we got the expected models
        assert_eq!(actual.len(), 2);
        insta::assert_json_snapshot!(actual);
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_models_http_error_status() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_error_response("Invalid API key", 401), 401)
            .await;

        let anthropic = create_anthropic(&fixture.url())?;
        let actual = anthropic.models().await;

        mock.assert_async().await;

        // Verify that we got an error
        assert!(actual.is_err());
        insta::assert_snapshot!(normalize_ports(format!("{:#?}", actual.unwrap_err())));
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_models_server_error() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_error_response("Internal Server Error", 500), 500)
            .await;

        let anthropic = create_anthropic(&fixture.url())?;
        let actual = anthropic.models().await;

        mock.assert_async().await;

        // Verify that we got an error
        assert!(actual.is_err());
        insta::assert_snapshot!(normalize_ports(format!("{:#?}", actual.unwrap_err())));

        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_models_empty_response() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture.mock_models(create_empty_response(), 200).await;

        let anthropic = create_anthropic(&fixture.url())?;
        let actual = anthropic.models().await?;

        mock.assert_async().await;
        assert!(actual.is_empty());
        Ok(())
    }
}
