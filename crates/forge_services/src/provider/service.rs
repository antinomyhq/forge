use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use forge_app::domain::{
    AnyProvider, ChatCompletionMessage, Context as ChatContext, HttpConfig, Model, ModelId,
    ProviderId, ResultStream, RetryConfig,
};
use forge_app::{EnvironmentInfra, HttpInfra, ProviderService};
use forge_domain::{Provider, ProviderRepository};
use tokio::sync::Mutex;
use url::Url;

use crate::http::HttpClient;
use crate::provider::client::{Client, ClientBuilder};

/// Flat cache structure for all models
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct FlatModelCache {
    models: Vec<Model>,
    #[serde(with = "systemtime_serde")]
    cached_at: SystemTime,
}

/// Custom serialization for SystemTime
mod systemtime_serde {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time
            .duration_since(UNIX_EPOCH)
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(UNIX_EPOCH + Duration::from_secs(secs))
    }
}

impl FlatModelCache {
    fn new(models: Vec<Model>) -> Self {
        Self { models, cached_at: SystemTime::now() }
    }

    /// Check if cache has expired based on TTL
    fn is_expired(&self, ttl: Duration) -> bool {
        SystemTime::now()
            .duration_since(self.cached_at)
            .map(|age| age > ttl)
            .unwrap_or(true)
    }
}

#[derive(Clone)]
pub struct ForgeProviderService<I> {
    retry_config: Arc<RetryConfig>,
    cached_clients: Arc<Mutex<HashMap<ProviderId, Client<HttpClient<I>>>>>,
    flat_cache: Arc<Mutex<Option<FlatModelCache>>>,
    cache_ttl: Duration,
    version: String,
    timeout_config: HttpConfig,
    infra: Arc<I>,
}

impl<I: EnvironmentInfra + HttpInfra> ForgeProviderService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.get_environment();
        let version = env.version();
        let cache_ttl = Duration::from_secs(env.model_cache_ttl_seconds);

        // Load flat cache from disk if available
        let flat_cache = Self::load_cache_from_disk(&env);

        let retry_config = Arc::new(env.retry_config);
        let timeout_config = env.http;

        Self {
            retry_config,
            cached_clients: Arc::new(Mutex::new(HashMap::new())),
            flat_cache: Arc::new(Mutex::new(flat_cache)),
            cache_ttl,
            version,
            timeout_config,
            infra,
        }
    }

    /// Create a new service with custom cache TTL
    pub fn with_cache_ttl(infra: Arc<I>, cache_ttl: Duration) -> Self {
        let mut service = Self::new(infra);
        service.cache_ttl = cache_ttl;
        service
    }

    /// Load flat cache from disk
    fn load_cache_from_disk(env: &forge_domain::Environment) -> Option<FlatModelCache> {
        let cache_path = env.model_cache_path();
        if !cache_path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&cache_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save flat cache to disk
    fn save_cache_to_disk(env: &forge_domain::Environment, cache: &FlatModelCache) {
        let cache_path = env.model_cache_path();

        // Ensure cache directory exists
        if let Some(parent) = cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Serialize and write cache
        if let Ok(content) = serde_json::to_string_pretty(cache) {
            let _ = std::fs::write(&cache_path, content);
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
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let client = self.client(provider).await?;

        client
            .chat(model, request)
            .await
            .with_context(|| format!("Failed to chat with model: {model}"))
    }

    async fn models(&self, provider: Provider<Url>) -> Result<Vec<Model>> {
        // Fetch models from client (no per-provider caching)
        let client = self.client(provider).await?;
        client.models().await
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

        // Invalidate flat cache since credentials changed
        self.invalidate_caches().await;

        Ok(())
    }

    async fn remove_credential(&self, id: &ProviderId) -> Result<()> {
        self.infra.remove_credential(id).await?;

        // Clear the cached client for this provider
        {
            let mut clients_guard = self.cached_clients.lock().await;
            clients_guard.remove(id);
        }

        // Invalidate flat cache since credentials removed
        self.invalidate_caches().await;

        Ok(())
    }

    async fn migrate_env_credentials(&self) -> Result<Option<forge_domain::MigrationResult>> {
        self.infra.migrate_env_credentials().await
    }

    async fn cache_all_models(&self, models: Vec<Model>) {
        let mut cache_guard = self.flat_cache.lock().await;
        *cache_guard = Some(FlatModelCache::new(models));

        // Save cache to disk
        if let Some(cache) = cache_guard.as_ref() {
            let env = self.infra.get_environment();
            Self::save_cache_to_disk(&env, cache);
        }
    }

    async fn get_cached_all_models(&self) -> Option<Vec<Model>> {
        let cache_guard = self.flat_cache.lock().await;
        cache_guard
            .as_ref()
            .filter(|cached| !cached.is_expired(self.cache_ttl))
            .map(|cached| cached.models.clone())
    }

    async fn invalidate_caches(&self) {
        let mut cache_guard = self.flat_cache.lock().await;
        *cache_guard = None;

        // Delete cache file from disk
        let env = self.infra.get_environment();
        let cache_path = env.model_cache_path();
        let _ = std::fs::remove_file(cache_path);
    }
}
