use std::sync::Arc;

use anyhow::Context as _;
use derive_setters::Setters;
use forge_app::HttpInfra;
use forge_app::domain::{
    ChatCompletionMessage, Context, Model, ModelId, ResultStream, RetryConfig,
};
use forge_app::dto::google::{EventData, Request};
use forge_domain::{ChatRepository, Provider};
use reqwest::Url;
use tokio_stream::StreamExt;
use tracing::debug;

use crate::provider::event::into_chat_completion_message;
use crate::provider::retry::into_retry;
use crate::provider::utils::{create_headers, format_http_context};

#[derive(Clone)]
struct Google<T> {
    http: Arc<T>,
    api_key: String,
    chat_url: Url,
    models: forge_domain::ModelSource<Url>,
}

impl<H: HttpInfra> Google<H> {
    pub fn new(
        http: Arc<H>,
        api_key: String,
        chat_url: Url,
        models: forge_domain::ModelSource<Url>,
    ) -> Self {
        Self { http, api_key, chat_url, models }
    }

    fn get_headers(&self) -> Vec<(String, String)> {
        vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Authorization".to_string(), format!("Bearer {}", self.api_key)),
        ]
    }
}

impl<T: HttpInfra> Google<T> {
    pub async fn chat(
        &self,
        model: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let request = Request::from(context);

        // Google models are specified in the URL path, not the request body
        // URL format: {base_url}/models/{model}:streamGenerateContent?alt=sse
        // The ?alt=sse query parameter is critical for proper SSE content-type
        let base_url = self.chat_url.as_str();
        let model_id_str = model.as_str();
        let model_name = model_id_str.strip_prefix("models/").unwrap_or(model_id_str);
        let full_url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse",
            base_url.trim_end_matches('/'),
            model_name
        );
        let url = Url::parse(&full_url)
            .with_context(|| "Failed to construct Google API URL")?;
        
        debug!(url = %url, model = %model, "Connecting Upstream");

        let json_bytes =
            serde_json::to_vec(&request).with_context(|| "Failed to serialize request")?;

        let source = self
            .http
            .http_eventsource(
                &url,
                Some(create_headers(self.get_headers())),
                json_bytes.into(),
            )
            .await
            .with_context(|| format_http_context(None, "POST", &url))?;

        let stream = into_chat_completion_message::<EventData>(url.clone(), source);

        Ok(Box::pin(stream))
    }

    pub async fn models(&self) -> anyhow::Result<Vec<Model>> {
        match &self.models {
            forge_domain::ModelSource::Url(url) => {
                debug!(url = %url, "Fetching models");

                let response = self
                    .http
                    .http_get(url, Some(create_headers(self.get_headers())))
                    .await
                    .with_context(|| format_http_context(None, "GET", url))
                    .with_context(|| "Failed to fetch models")?;

                let status = response.status();
                let ctx_msg = format_http_context(Some(status), "GET", url);
                let text = response
                    .text()
                    .await
                    .with_context(|| ctx_msg.clone())
                    .with_context(|| "Failed to decode response into text")?;

                if status.is_success() {
                    // Google's models endpoint returns { "models": [...] }
                    #[derive(serde::Deserialize)]
                    struct ModelsResponse {
                        models: Vec<forge_app::dto::google::Model>,
                    }

                    let response: ModelsResponse = serde_json::from_str(&text)
                        .with_context(|| ctx_msg)
                        .with_context(|| "Failed to deserialize models response")?;
                    Ok(response.models.into_iter().map(Into::into).collect())
                } else {
                    // treat non 200 response as error.
                    Err(anyhow::anyhow!(text))
                        .with_context(|| ctx_msg)
                        .with_context(|| "Failed to fetch the models")
                }
            }
            forge_domain::ModelSource::Hardcoded(models) => {
                debug!("Using hardcoded models");
                Ok(models.clone())
            }
        }
    }
}

/// Repository for Google provider responses
#[derive(Setters)]
#[setters(strip_option, into)]
pub struct GoogleResponseRepository<F> {
    infra: Arc<F>,
    retry_config: Arc<RetryConfig>,
}

impl<F> GoogleResponseRepository<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, retry_config: Arc::new(RetryConfig::default()) }
    }
}

impl<F: HttpInfra> GoogleResponseRepository<F> {
    /// Creates a Google client from a provider configuration
    fn create_client(&self, provider: &Provider<Url>) -> anyhow::Result<Google<F>> {
        let chat_url = provider.url.clone();
        let models = provider
            .models
            .clone()
            .context("Google requires models configuration")?;
        let creds = provider
            .credential
            .as_ref()
            .context("Google provider requires credentials")?
            .auth_details
            .clone();

        // For Vertex AI, the Google ADC token is stored as ApiKey
        // For OAuth, extract the access token
        let token = match creds {
            forge_domain::AuthDetails::ApiKey(api_key) => api_key.as_str().to_string(),
            forge_domain::AuthDetails::OAuth { tokens, .. } => tokens.access_token.as_str().to_string(),
            forge_domain::AuthDetails::OAuthWithApiKey { api_key, .. } => api_key.as_str().to_string(),
        };

        Ok(Google::new(self.infra.clone(), token, chat_url, models))
    }
}

#[async_trait::async_trait]
impl<F: HttpInfra + 'static> ChatRepository for GoogleResponseRepository<F> {
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let retry_config = self.retry_config.clone();
        let provider_client = self.create_client(&provider)?;

        let stream = provider_client
            .chat(model_id, context)
            .await
            .map_err(|e| into_retry(e, &retry_config))?;

        Ok(Box::pin(stream.map(move |item| {
            item.map_err(|e| into_retry(e, &retry_config))
        })))
    }

    async fn models(&self, provider: Provider<Url>) -> anyhow::Result<Vec<Model>> {
        let retry_config = self.retry_config.clone();
        let provider_client = self.create_client(&provider)?;

        provider_client
            .models()
            .await
            .map_err(|e| into_retry(e, &retry_config))
            .context("Failed to fetch models from Google provider")
    }
}
