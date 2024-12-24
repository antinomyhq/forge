use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use super::model_response::ListModelResponse;
use super::request::Request;
use super::response::Response;
use crate::error::Result;
use crate::provider::{InnerProvider, Provider};
use crate::{Error, ProviderError, ResultStream};

const DEFAULT_MODEL: &str = "openai/gpt-4o-mini";
const PROVIDER_NAME: &str = "Open Router";

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
    client: Client,
    config: Config,
    #[allow(unused)]
    model: String,
}

impl OpenRouter {
    fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        let config = Config { api_key, base_url };

        let client = Client::builder().build().unwrap();

        Self {
            client,
            config,
            model: model.unwrap_or(DEFAULT_MODEL.to_string()),
        }
    }
}

#[async_trait::async_trait]
impl InnerProvider for OpenRouter {
    async fn chat(
        &self,
        body: crate::model::Request,
    ) -> Result<ResultStream<crate::model::Response>> {
        let mut new_body = Request::from(body);

        new_body.model = self.model.clone();
        new_body.stream = Some(true);

        let body = serde_json::to_string(&new_body)?;

        tracing::debug!("Request Body: {}", body);

        let rb = self
            .client
            .post(self.config.url("/chat/completions"))
            .headers(self.config.headers())
            .body(body);

        let mut es = EventSource::new(rb).unwrap();

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<crate::Response>>(100);
        while let Some(event) = es.next().await {
            match event {
                Ok(event) => match event {
                    Event::Open => {}
                    Event::Message(event) => {
                        if event.data == "[DONE]" {
                            break;
                        }

                        match serde_json::from_str::<Response>(&event.data) {
                            Ok(response) => {
                                let response = crate::Response::try_from(response);
                                tx.send(response).await.unwrap();
                            }
                            Err(_) => {
                                let value: serde_json::Value =
                                    serde_json::from_str(&event.data).unwrap();

                                tx.send(Err(Error::Provider {
                                    provider: PROVIDER_NAME.to_string(),
                                    error: ProviderError::UpstreamError(value),
                                }))
                                .await
                                .unwrap();
                                break;
                            }
                        }
                    }
                },
                Err(err) => {
                    tx.send(Err(err.into())).await.unwrap();
                    break;
                }
            }
        }

        let processed_stream = ReceiverStream::new(rx);

        Ok(Box::pin(Box::new(processed_stream)))
    }

    async fn models(&self) -> Result<Vec<String>> {
        let text = self
            .client
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
