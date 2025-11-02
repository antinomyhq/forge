use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{
    ChatCompletionMessage, Context as ChatContext, HttpConfig, Model, ModelId, Provider,
    ProviderId, ResultStream, RetryConfig,
};
use forge_app::{EnvironmentInfra, HttpInfra, ProviderService};
use forge_domain::ProviderRepository;
use tokio::sync::Mutex;

use crate::http::HttpClient;
use crate::provider::client::{Client, ClientBuilder};
#[derive(Clone)]
pub struct ForgeProviderService<I> {
    retry_config: Arc<RetryConfig>,
    cached_clients: Arc<Mutex<HashMap<ProviderId, Client<HttpClient<I>>>>>,
    cached_models: Arc<Mutex<HashMap<ProviderId, Vec<Model>>>>,
    version: String,
    timeout_config: HttpConfig,
    http_infra: Arc<I>,
}

impl<I: EnvironmentInfra + HttpInfra> ForgeProviderService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.get_environment();
        let version = env.version();
        let retry_config = Arc::new(env.retry_config);
        Self {
            retry_config,
            cached_clients: Arc::new(Mutex::new(HashMap::new())),
            cached_models: Arc::new(Mutex::new(HashMap::new())),
            version,
            timeout_config: env.http,
            http_infra: infra,
        }
    }

    async fn client(&self, provider: Provider) -> Result<Client<HttpClient<I>>> {
        let provider_id = provider.id;

        // Check cache first
        {
            let clients_guard = self.cached_clients.lock().await;
            if let Some(cached_client) = clients_guard.get(&provider_id) {
                return Ok(cached_client.clone());
            }
        }

        // Client not in cache, create new client
        let infra = self.http_infra.clone();
        let client = ClientBuilder::new(provider, &self.version)
            .retry_config(self.retry_config.clone())
            .timeout_config(self.timeout_config.clone())
            .use_hickory(false) // use native DNS resolver(GAI)
            .build(Arc::new(HttpClient::new(infra)))?;

        // Cache the new client for this provider
        {
            let mut clients_guard = self.cached_clients.lock().await;
            clients_guard.insert(provider_id, client.clone());
        }

        Ok(client)
    }
}

#[async_trait::async_trait]
impl<I: EnvironmentInfra + HttpInfra + ProviderRepository> ProviderService
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

    async fn get_provider(&self, id: ProviderId) -> Result<Provider> {
        self.http_infra.get_provider(id).await
    }

    async fn get_all_providers(&self) -> Result<Vec<Provider>> {
        self.http_infra.get_all_providers().await
    }
}
