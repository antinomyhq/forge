use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::Context as _;
use derive_setters::Setters;
use forge_app::HttpInfra;
use forge_app::domain::{
    ChatCompletionMessage, Context, Model, ModelId, Provider, ResultStream, RetryConfig,
};
use forge_domain::ChatRepository;
use tokio_stream::StreamExt;
use url::Url;

use crate::provider_client::anthropic::Anthropic;
use crate::provider_client::retry::into_retry;

/// Repository for Anthropic provider responses
#[derive(Setters)]
#[setters(strip_option, into)]
pub struct AnthropicResponseRepository<F> {
    infra: Arc<F>,
    retry_config: Arc<RetryConfig>,
    _phantom: PhantomData<F>,
}

impl<F> AnthropicResponseRepository<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self {
            infra,
            retry_config: Arc::new(RetryConfig::default()),
            _phantom: PhantomData,
        }
    }
}

impl<F: HttpInfra> AnthropicResponseRepository<F> {
    /// Creates an Anthropic client from a provider configuration
    fn create_client(&self, provider: &Provider<Url>) -> anyhow::Result<Anthropic<F>> {
        let api_key = provider
            .api_key()
            .context("Anthropic requires an API key")?
            .as_str()
            .to_string();
        let chat_url = provider.url.clone();
        let models = provider
            .models
            .clone()
            .context("Anthropic requires models configuration")?;

        Ok(Anthropic::new(
            self.infra.clone(),
            api_key,
            chat_url,
            models,
            "2023-06-01".to_string(),
            false,
        ))
    }
}

#[async_trait::async_trait]
impl<F: HttpInfra + 'static> ChatRepository for AnthropicResponseRepository<F> {
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
            .context("Failed to fetch models from Anthropic provider")
    }
}
