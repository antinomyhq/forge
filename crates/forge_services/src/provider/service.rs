use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{
    AnyProvider, ChatCompletionMessage, Context as ChatContext, Environment, Model, ModelId,
    ProviderId, ResultStream,
};
use forge_app::{EnvironmentInfra, HttpInfra, ProviderService};
use forge_domain::{Provider, ProviderRepository};
use tokio::sync::Mutex;
use url::Url;

use crate::http::HttpClient;
use crate::provider::client::{Client, ClientBuilder};

#[derive(Clone)]
pub struct ForgeProviderService<I> {
    env: Environment,
    cached_clients: Arc<Mutex<HashMap<ProviderId, Client<HttpClient<I>>>>>,
    cached_models: Arc<Mutex<HashMap<ProviderId, Vec<Model>>>>,
    infra: Arc<I>,
}

impl<I: EnvironmentInfra + HttpInfra> ForgeProviderService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.get_environment();
        Self {
            env,
            cached_clients: Arc::new(Mutex::new(HashMap::new())),
            cached_models: Arc::new(Mutex::new(HashMap::new())),
            infra,
        }
    }
}

impl<I: EnvironmentInfra + HttpInfra> ForgeProviderService<I> {
    async fn client(&self, provider: Provider<Url>) -> Result<Client<HttpClient<I>>> {
        let provider_id = provider.id.clone();

        // Check cache first
        {
            let clients_guard = self.cached_clients.lock().await;
            if let Some(cached_client) = clients_guard.get(&provider_id) {
                return Ok(cached_client.clone());
            }
        }

        // Client not in cache, create new client
        let infra = self.infra.clone();
        let client = ClientBuilder::new(provider, &self.env)
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
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let client = self.client(provider).await?;

        client
            .chat(model, request)
            .await
            .with_context(|| format!("Failed to chat with model: {model}"))
    }

    async fn models(&self, provider: Provider<Url>) -> Result<Vec<Model>> {
        let provider_id = provider.id.clone();

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

    async fn get_provider(&self, id: ProviderId) -> Result<Provider<Url>> {
        self.infra.get_provider(id).await
    }

    async fn get_all_providers(&self) -> Result<Vec<AnyProvider>> {
        self.infra.get_all_providers().await
    }

    async fn upsert_credential(&self, credential: forge_domain::AuthCredential) -> Result<()> {
        let provider_id = credential.id.clone();

        // Save the credential to the repository
        self.infra.upsert_credential(credential).await?;

        // Clear the cached client for this provider to force recreation with new
        // credentials
        {
            let mut clients_guard = self.cached_clients.lock().await;
            clients_guard.remove(&provider_id);
        }

        Ok(())
    }

    async fn remove_credential(&self, id: &ProviderId) -> Result<()> {
        self.infra.remove_credential(id).await?;

        // Clear the cached client for this provider
        {
            let mut clients_guard = self.cached_clients.lock().await;
            clients_guard.remove(id);
        }

        Ok(())
    }

    async fn migrate_env_credentials(&self) -> Result<Option<forge_domain::MigrationResult>> {
        self.infra.migrate_env_credentials().await
    }
}
