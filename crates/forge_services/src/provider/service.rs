use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::ProviderService;
use forge_app::domain::{
    ChatCompletionMessage, Context as ChatContext, HttpConfig, Model, ModelId, ResultStream,
    RetryConfig,
};
use forge_app::dto::{Provider, ProviderId};
use tokio::sync::Mutex;

use crate::http::HttpClient;
use crate::infra::HttpInfra;
use crate::provider::client::{Client, ClientBuilder};
use crate::{AppConfigRepository, EnvironmentInfra};
#[derive(Clone)]
pub struct ForgeProviderService<I> {
    retry_config: Arc<RetryConfig>,
    cached_client: Arc<Mutex<Option<Client<HttpClient<I>>>>>,
    cached_models: Arc<Mutex<HashMap<ProviderId, Vec<Model>>>>,
    version: String,
    timeout_config: HttpConfig,
    http_infra: Arc<I>,
}

impl<I: EnvironmentInfra + HttpInfra + AppConfigRepository> ForgeProviderService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.get_environment();
        let version = env.version();
        let retry_config = Arc::new(env.retry_config);
        Self {
            retry_config,
            cached_client: Arc::new(Mutex::new(None)),
            cached_models: Arc::new(Mutex::new(HashMap::new())),
            version,
            timeout_config: env.http,
            http_infra: infra,
        }
    }

    async fn client(&self, provider: Provider) -> Result<Client<HttpClient<I>>> {
        let mut client_guard = self.cached_client.lock().await;

        match client_guard.as_ref() {
            Some(client) => Ok(client.clone()),
            None => {
                let infra = self.http_infra.clone();
                let client = ClientBuilder::new(provider, &self.version)
                    .retry_config(self.retry_config.clone())
                    .timeout_config(self.timeout_config.clone())
                    .use_hickory(false) // use native DNS resolver(GAI)
                    .build(Arc::new(HttpClient::new(infra)))?;

                // Cache the new client
                *client_guard = Some(client.clone());
                Ok(client)
            }
        }
    }
}

#[async_trait::async_trait]
impl<I: EnvironmentInfra + HttpInfra + AppConfigRepository> ProviderService
    for ForgeProviderService<I>
{
    async fn chat(
        &self,
        model: &ModelId,
        request: ChatContext,
        provider: Provider,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let client = self.client(provider).await?;

        client
            .chat(model, request)
            .await
            .with_context(|| format!("Failed to chat with model: {model}"))
    }

    async fn models(&self, provider: Provider) -> Result<Vec<Model>> {
        let provider_id = provider.id;

        // Check cache first
        {
            let models_guard = self.cached_models.lock().await;
            if let Some(cached_models) = models_guard.get(&provider_id) {
                return Ok(cached_models.clone());
            }
        }

        // Models not in cache, fetch from client
        let client = self.client(provider).await?;
        let models = client.models().await?;

        // Cache the models for this provider
        {
            let mut models_guard = self.cached_models.lock().await;
            models_guard.insert(provider_id, models.clone());
        }

        Ok(models)
    }
}
