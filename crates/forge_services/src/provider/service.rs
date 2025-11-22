use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{
    AnyProvider, ChatCompletionMessage, Context as ChatContext, HttpConfig, Model, ModelId,
    ProviderId, ResultStream, RetryConfig,
};
use forge_app::{EnvironmentInfra, HttpInfra, ProviderService};
use forge_domain::{AuthCredential, CredentialsRepository, Provider, ProviderRepository};
use tokio::sync::Mutex;
use url::Url;

use crate::http::HttpClient;
use crate::provider::client::{Client, ClientBuilder};
#[derive(Clone)]
pub struct ForgeProviderService<I> {
    retry_config: Arc<RetryConfig>,
    cached_clients: Arc<Mutex<HashMap<ProviderId, Client<HttpClient<I>>>>>,
    cached_models: Arc<Mutex<HashMap<ProviderId, Vec<Model>>>>,
    version: String,
    timeout_config: HttpConfig,
    infra: Arc<I>,
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
            infra,
        }
    }
}

impl<I: EnvironmentInfra + HttpInfra + CredentialsRepository> ForgeProviderService<I> {
    /// Populate ForgeServices credentials from SQLite into provider list
    async fn populate_forge_services_credentials(
        &self,
        providers: &mut [AnyProvider],
    ) -> Result<()> {
        use forge_domain::AnyProvider;

        // Find ForgeServices provider position
        let position = providers
            .iter()
            .position(|p| p.id() == ProviderId::ForgeServices);

        let Some(position) = position else {
            return Ok(());
        };

        // Fetch credentials from SQLite
        let Some(indexing_auth) = self.infra.get_auth().await? else {
            return Ok(());
        };

        // Create credential from indexing auth
        let credential = AuthCredential {
            id: ProviderId::ForgeServices,
            auth_details: indexing_auth.into(),
            url_params: std::collections::HashMap::new(),
        };

        // Update provider with credential
        let provider = &providers[position];
        providers[position] = match provider {
            AnyProvider::Url(p) => {
                let mut updated = p.clone();
                updated.credential = Some(credential);
                AnyProvider::Url(updated)
            }
            AnyProvider::Template(p) => {
                // Convert Template to Url provider
                let url = Url::parse(&p.url.template)
                    .map_err(|e| anyhow::anyhow!("Failed to parse ForgeServices URL: {}", e))?;

                AnyProvider::Url(Provider {
                    id: p.id,
                    provider_type: p.provider_type,
                    response: p.response.clone(),
                    url: url.clone(),
                    auth_methods: p.auth_methods.clone(),
                    url_params: p.url_params.clone(),
                    credential: Some(credential),
                    models: p.models.clone().and_then(|m| match m {
                        forge_domain::ModelSource::Url(t) => {
                            Url::parse(&t.template).ok().map(forge_domain::ModelSource::Url)
                        }
                        forge_domain::ModelSource::Hardcoded(list) => {
                            Some(forge_domain::ModelSource::Hardcoded(list))
                        }
                    }),
                })
            }
        };

        Ok(())
    }

    async fn client(&self, provider: Provider<Url>) -> Result<Client<HttpClient<I>>> {
        let provider_id = provider.id;

        // Check cache first
        {
            let clients_guard = self.cached_clients.lock().await;
            if let Some(cached_client) = clients_guard.get(&provider_id) {
                return Ok(cached_client.clone());
            }
        }

        // Client not in cache, create new client
        let infra = self.infra.clone();
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
impl<I: EnvironmentInfra + HttpInfra + ProviderRepository + CredentialsRepository> ProviderService
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

    async fn get_provider(&self, id: ProviderId) -> Result<Provider<Url>> {
        self.infra.get_provider(id).await
    }

    async fn get_all_providers(&self) -> Result<Vec<AnyProvider>> {
        let mut providers = self.infra.get_all_providers().await?;

        // Populate ForgeServices credentials from SQLite
        self.populate_forge_services_credentials(&mut providers)
            .await?;

        Ok(providers)
    }

    async fn upsert_credential(&self, credential: forge_domain::AuthCredential) -> Result<()> {
        let provider_id = credential.id;

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
