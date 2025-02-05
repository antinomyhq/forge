use anyhow::{Context as _, Result};
use derive_setters::Setters;
use forge_domain::{
    self, ChatCompletionMessage, Context as ChatContext, Model, ModelId, Parameters,
    ProviderService, ResultStream,
};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{Client, Url};
use reqwest_eventsource::{Event, RequestBuilderExt};
use tokio_stream::StreamExt;

use super::model::{ListModelResponse, OpenRouterModel};
use super::parameters::ParameterResponse;
use super::request::OpenRouterRequest;
use super::response::OpenRouterResponse;

#[derive(Debug, Default, Clone, Setters)]
#[setters(into, strip_option)]
pub struct OpenRouterBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
}

impl OpenRouterBuilder {
    pub fn build(self) -> anyhow::Result<OpenRouter> {
        let client = Client::builder().build()?;
        let base_url = self
            .base_url
            .as_deref()
            .unwrap_or("http://localhost:11434/v1/");

        let base_url = Url::parse(base_url)
            .with_context(|| format!("Failed to parse base URL: {}", base_url))?;

        Ok(OpenRouter { client, base_url, api_key: self.api_key })
    }
}

#[derive(Clone)]
pub struct OpenRouter {
    client: Client,
    api_key: Option<String>,
    base_url: Url,
}

impl OpenRouter {
    pub fn builder() -> OpenRouterBuilder {
        OpenRouterBuilder::default()
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

        if let Some(ref api_key) = self.api_key {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap(),
            );
        }
        headers.insert("X-Title", HeaderValue::from_static("code-forge"));
        headers
    }
}

#[async_trait::async_trait]
impl ProviderService for OpenRouter {
    async fn chat(
        &self,
        model_id: &ModelId,
        request: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let request = OpenRouterRequest::from(request)
            .model(model_id.clone())
            .stream(true)
            .cache();

        let es = self
            .client
            .post(self.url("chat/completions")?)
            .headers(self.headers())
            .json(&request)
            .eventsource()?;

        let stream = es
            .take_while(|message| !matches!(message, Err(reqwest_eventsource::Error::StreamEnded)))
            .then(|event| async {
                match event {
                    Ok(event) => match event {
                        Event::Open => None,
                        Event::Message(event) if ["[DONE]", ""].contains(&event.data.as_str()) => {
                            None
                        }
                        Event::Message(event) => {
                            println!("{}", serde_json::from_str::<serde_json::Value>(&event.data).unwrap());
                            Some(
                                serde_json::from_str::<OpenRouterResponse>(&event.data)
                                    .with_context(|| "Failed to parse OpenRouter response")
                                    .and_then(|message| {
                                        ChatCompletionMessage::try_from(message.clone())
                                            .with_context(|| "Failed to create completion message")
                                    }),
                            )
                        },
                    },
                    Err(reqwest_eventsource::Error::StreamEnded) => None,
                    Err(reqwest_eventsource::Error::InvalidStatusCode(code, response)) => {
                        println!("Invalid status code: {}", code);
                        let x = response.text().await;
                        println!("Response: {:?}", x);
                        let x = x.map_err(|e| anyhow::anyhow!("{}", e)).and_then(|x| {
                            serde_json::from_str::<OpenRouterResponse>(&x)
                                .with_context(|| "Failed to parse OpenRouter response")
                        });

                        Some(
                            x
                                .with_context(|| "Failed to parse OpenRouter response")
                                .and_then(|message| {
                                    ChatCompletionMessage::try_from(message.clone())
                                        .with_context(|| "Failed to create completion message")
                                })
                                .with_context(|| "Failed with invalid status code"),
                        )
                    }
                    Err(reqwest_eventsource::Error::InvalidContentType(_, response)) => Some(
                        response
                            .json::<OpenRouterResponse>()
                            .await
                            .with_context(|| "Failed to parse OpenRouter response")
                            .and_then(|message| {
                                ChatCompletionMessage::try_from(message.clone())
                                    .with_context(|| "Failed to create completion message")
                            })
                            .with_context(|| "Failed with invalid content type"),
                    ),
                    Err(err) => Some(Err(err.into())),
                }
            });

        Ok(Box::pin(stream.filter_map(|x| x)))
    }

    async fn models(&self) -> Result<Vec<Model>> {
        let text = self
            .client
            .get(self.url("models")?)
            .headers(self.headers())
            .send()
            .await?
            .error_for_status()
            .with_context(|| "Failed because of a non 200 status code".to_string())?
            .text()
            .await?;

        let response: ListModelResponse = serde_json::from_str(&text)?;

        Ok(response
            .data
            .iter()
            .map(|r| r.clone().into())
            .collect::<Vec<Model>>())
    }

    async fn parameters(&self, model: &ModelId) -> Result<Parameters> {
        Ok(Parameters { tool_supported: true })

        /*
        // For Eg: https://openrouter.ai/api/v1/parameters/google/gemini-pro-1.5-exp
        let path = format!("parameters/{}", model.as_str());

        let url = self.url(&path)?;

        let text = self
            .client
            .get(url)
            .headers(self.headers())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let response: ParameterResponse = serde_json::from_str(&text)
            .with_context(|| "Failed to parse parameter response".to_string())?;

        Ok(Parameters {
            tool_supported: response
                .data
                .supported_parameters
                .iter()
                .flat_map(|parameter| parameter.iter())
                .any(|parameter| parameter == "tools"),
        })*/
    }
}

impl From<OpenRouterModel> for Model {
    fn from(value: OpenRouterModel) -> Self {
        Model {
            id: value.id,
            name: value.name,
            description: value.description,
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use reqwest::Url;

    use super::*;

    fn create_test_client() -> OpenRouter {
        OpenRouter {
            client: Client::new(),
            api_key: None,
            base_url: Url::parse("https://openrouter.ai/api/v1/").unwrap(),
        }
    }

    #[test]
    fn test_url_basic_path() -> Result<()> {
        let client = create_test_client();
        let url = client.url("chat/completions")?;
        assert_eq!(
            url.as_str(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
        Ok(())
    }

    #[test]
    fn test_url_with_leading_slash() -> Result<()> {
        let client = create_test_client();
        // Remove leading slash since base_url already ends with a slash
        let path = "chat/completions".trim_start_matches('/');
        let url = client.url(path)?;
        assert_eq!(
            url.as_str(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
        Ok(())
    }

    #[test]
    fn test_url_with_special_characters() -> Result<()> {
        let client = create_test_client();
        let url = client.url("parameters/google/gemini-pro-1.5-exp")?;
        assert_eq!(
            url.as_str(),
            "https://openrouter.ai/api/v1/parameters/google/gemini-pro-1.5-exp"
        );
        Ok(())
    }

    #[test]
    fn test_url_with_empty_path() -> Result<()> {
        let client = create_test_client();
        let url = client.url("")?;
        assert_eq!(url.as_str(), "https://openrouter.ai/api/v1/");
        Ok(())
    }

    #[test]
    fn test_url_with_invalid_path() {
        let client = create_test_client();
        let result = client.url("https://malicious.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_url_with_directory_traversal() {
        let client = create_test_client();
        let result = client.url("../invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_deserialization() -> Result<()> {
        let content = serde_json::to_string(&serde_json::json!({
          "error": {
            "message": "This endpoint's maximum context length is 16384 tokens",
            "code": 400
          }
        }))
            .unwrap();
        let message = serde_json::from_str::<OpenRouterResponse>(&content)
            .context("Failed to parse response")?;
        let message = ChatCompletionMessage::try_from(message.clone());

        assert!(message.is_err());
        Ok(())
    }
}
