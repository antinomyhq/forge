use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::{ChatService, EnvironmentService, KeyService};
use forge_domain::{
    ChatCompletionMessage, Context as ChatContext, ForgeKey, Model, ModelId, Provider,
    ResultStream, RetryConfig,
};
use forge_provider::Client;
use tokio::sync::RwLock;

use crate::{Infrastructure, ProviderService};

#[derive(Clone)]
pub struct ForgeProviderService<I, K> {
    infra: Arc<I>,
    key_service: Arc<K>,
    retry_config: Arc<RetryConfig>,
    cached_client: Arc<RwLock<Option<Client>>>,
    version: String,
}

impl<K: KeyService, I: Infrastructure> ForgeProviderService<I, K> {
    pub fn new(infra: Arc<I>, key_service: Arc<K>) -> Self {
        let env = infra.environment_service().get_environment();
        let version = env.version();
        let retry_config = Arc::new(env.retry_config);
        Self {
            key_service,
            infra,
            retry_config,
            cached_client: Arc::new(RwLock::new(None)),
            version,
        }
    }
    async fn key(&self) -> Result<ForgeKey> {
        let forge_key = self
            .key_service
            .get()
            .await
            .context("User isn't logged in")?;
        Ok(forge_key)
    }
    async fn provider(&self) -> Result<Provider> {
        let key = self.key().await?;

        self.infra
            .provider_service()
            .get(Some(key))
            .context("User isn't logged in")
    }

    async fn client(&self) -> Result<Client> {
        {
            let client_guard = self.cached_client.read().await;
            if let Some(client) = client_guard.as_ref() {
                return Ok(client.clone());
            }
        }

        // Client doesn't exist, create new one
        let provider = self.provider().await?;
        let client = Client::new(provider, self.retry_config.clone(), &self.version)?;

        // Cache the new client
        {
            let mut client_guard = self.cached_client.write().await;
            *client_guard = Some(client.clone());
        }

        Ok(client)
    }
}

#[async_trait::async_trait]
impl<K: KeyService, I: Infrastructure> ChatService for ForgeProviderService<I, K> {
    async fn chat(
        &self,
        model: &ModelId,
        request: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let client = self.client().await?;

        client
            .chat(model, request)
            .await
            .with_context(|| format!("Failed to chat with model: {model}"))
    }

    async fn models(&self) -> Result<Vec<Model>> {
        let client = self.client().await?;

        client.models().await
    }
}
