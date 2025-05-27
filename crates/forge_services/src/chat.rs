use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{
    ChatCompletionMessage, ChatService, Context as ChatContext, EnvironmentService, ForgeKey,
    KeyService, Model, ModelId, Provider, ResultStream,
};
use forge_provider::Client;
use tokio::sync::RwLock;

use crate::{Infrastructure, ProviderService};

#[derive(Clone)]
pub struct ForgeProviderService<I, K> {
    infra: Arc<I>,
    key_service: Arc<K>,
    retry_status_codes: Arc<Vec<u16>>,
    cached_client: Arc<RwLock<Option<Client>>>,
}

impl<K: KeyService, I: Infrastructure> ForgeProviderService<I, K> {
    pub fn new(infra: Arc<I>, key_service: Arc<K>) -> Self {
        let env = infra.environment_service().get_environment();
        let retry_status_codes = Arc::new(env.retry_config.retry_status_codes);
        Self {
            key_service,
            infra,
            retry_status_codes,
            cached_client: Arc::new(RwLock::new(None)),
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
        let client = Client::new(provider, self.retry_status_codes.clone())?;

        // Cache the new client
        {
            let mut client_guard = self.cached_client.write().await;
            *client_guard = Some(client.clone());
        }

        Ok(client)
    }

    /// Invalidates the cached client, forcing a new one to be created on next
    /// use
    pub async fn invalidate_client_cache(&self) {
        let mut client_guard = self.cached_client.write().await;
        *client_guard = None;
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

    async fn model(&self, model: &ModelId) -> Result<Option<Model>> {
        let client = self.client().await?;

        client.model(model).await
    }
}
