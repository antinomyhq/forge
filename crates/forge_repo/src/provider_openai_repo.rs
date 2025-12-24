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

use crate::provider_client::openai::OpenAIProvider;
use crate::provider_client::retry::into_retry;

/// Repository for OpenAI-compatible provider responses
///
/// Handles providers that use OpenAI's API format including:
/// - OpenAI
/// - Azure OpenAI
/// - Vertex AI
/// - OpenRouter
/// - DeepSeek
/// - Groq
#[derive(Setters)]
#[setters(strip_option, into)]
pub struct OpenAIResponseRepository<F> {
    infra: Arc<F>,
    retry_config: Arc<RetryConfig>,
    _phantom: PhantomData<F>,
}

impl<F> OpenAIResponseRepository<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self {
            infra,
            retry_config: Arc::new(RetryConfig::default()),
            _phantom: PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<F: HttpInfra + 'static> ChatRepository for OpenAIResponseRepository<F> {
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let retry_config = self.retry_config.clone();
        let provider_client = OpenAIProvider::new(provider, self.infra.clone());
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
        let provider_client = OpenAIProvider::new(provider, self.infra.clone());
        provider_client
            .models()
            .await
            .map_err(|e| into_retry(e, &retry_config))
            .context("Failed to fetch models from OpenAI-compatible provider")
    }
}
