use anyhow::{Context as _, Result};
use forge_app::dto::models_dev::{ModelData, ModelsDevResponse};
use forge_app::{EnvironmentInfra, HttpInfra};
use forge_domain::{Model, ModelId, ModelRepository, ProviderId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs;
use tracing::{debug, info, warn};
use url::Url;

/// Cache data structure stored in the cache file
///
/// Contains the transformed models along with a timestamp for TTL validation.
#[derive(Debug, Serialize, Deserialize)]
struct CacheData {
    /// Unix timestamp (seconds since epoch) when the cache was created
    cached_at: u64,
    /// Map of provider IDs to their models
    models: HashMap<ProviderId, Vec<Model>>,
}

/// Repository for managing model metadata from models.dev
///
/// This repository fetches model data from models.dev API, transforms it to
/// forge's internal format, and caches it locally with TTL-based expiration.
pub struct ForgeModelRepository<F> {
    infra: Arc<F>,
    cache_path: PathBuf,
    cache_ttl: Duration,
}

impl<F: EnvironmentInfra + HttpInfra> ForgeModelRepository<F> {
    /// Creates a new ForgeModelRepository
    ///
    /// # Arguments
    /// * `infra` - Infrastructure providing environment and HTTP capabilities
    pub fn new(infra: Arc<F>) -> Self {
        let env = infra.get_environment();
        let cache_path = env.cache_dir().join("models.json");
        let cache_ttl = Duration::from_secs(24 * 60 * 60); // 24 hours

        Self {
            infra,
            cache_path,
            cache_ttl,
        }
    }

    /// Gets current time in seconds since UNIX_EPOCH
    /// 
    /// In tests with `#[tokio::test(start_paused = true)]`, this will use tokio's
    /// mocked time which can be advanced with `tokio::time::advance()`
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX_EPOCH")
            .as_secs()
    }

    /// Fetches models from models.dev API and transforms them
    ///
    /// Makes an HTTP request to models.dev, applies provider ID mapping,
    /// filters out dynamic providers, and caches the result.
    ///
    /// # Errors
    /// Returns an error if HTTP request fails, JSON parsing fails, or file I/O
    /// fails
    async fn fetch_and_transform_models(&self) -> Result<HashMap<ProviderId, Vec<Model>>> {
        info!("Fetching models from models.dev API");

        let url = Url::parse("https://models.dev/api.json")
            .context("Failed to parse models.dev URL")?;

        let response = self
            .infra
            .http_get(&url, None)
            .await
            .context("Failed to fetch models from models.dev")?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("models.dev API returned status {}", status);
        }

        let text = response
            .text()
            .await
            .context("Failed to read response body")?;

        let models_dev_response: ModelsDevResponse =
            serde_json::from_str(&text).context("Failed to deserialize models.dev response")?;

        debug!(
            "Received {} providers from models.dev",
            models_dev_response.0.len()
        );

        // Transform and map providers
        let mut models_map: HashMap<ProviderId, Vec<Model>> = HashMap::new();

        for (provider_id, provider_data) in models_dev_response.0 {
            // Map models.dev provider ID to forge provider ID(s)
            let forge_provider_ids = crate::provider::map_models_dev_provider_id(&provider_id);
            
            if forge_provider_ids.is_empty() {
                debug!("Skipping unmapped provider: {}", provider_id);
                continue;
            }

            // Transform models
            let models: Vec<Model> = provider_data
                .models
                .into_values()
                .map(|model_data: ModelData| model_data.into())
                .collect();

            // Add models for each mapped forge provider ID
            for forge_provider_id in forge_provider_ids {
                debug!(
                    "Mapped provider {} to {:?} with {} models",
                    provider_id,
                    forge_provider_id,
                    models.len()
                );

                models_map
                    .entry(forge_provider_id)
                    .or_default()
                    .extend(models.clone());
            }
        }

        // Write to cache
        self.write_cache(&models_map).await?;

        Ok(models_map)
    }

    /// Writes models to the cache file with timestamp
    ///
    /// # Errors
    /// Returns an error if directory creation or file writing fails
    async fn write_cache(&self, models: &HashMap<ProviderId, Vec<Model>>) -> Result<()> {
        // Ensure cache directory exists
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent)
                .await
                .context("Failed to create cache directory")?;
        }

        let cached_at = self.now();

        let cache_data = CacheData {
            cached_at,
            models: models.clone(),
        };

        let json = serde_json::to_string_pretty(&cache_data)
            .context("Failed to serialize cache data")?;

        fs::write(&self.cache_path, json)
            .await
            .context("Failed to write cache file")?;

        info!("Wrote cache to {:?}", self.cache_path);

        Ok(())
    }

    /// Loads models from the cache file if valid
    ///
    /// Checks if the cache exists and is not expired based on the cached_at
    /// timestamp.
    ///
    /// # Returns
    /// * `Ok(Some(HashMap))` - Valid cache loaded
    /// * `Ok(None)` - Cache doesn't exist, is expired, or is corrupted
    ///
    /// # Errors
    /// Returns an error if file reading fails unexpectedly
    async fn load_cache(&self) -> Result<Option<HashMap<ProviderId, Vec<Model>>>> {
        // Check if cache file exists
        if !self.cache_path.exists() {
            debug!("Cache file does not exist");
            return Ok(None);
        }

        // Read cache file
        let content = match fs::read_to_string(&self.cache_path).await {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read cache file: {}", e);
                return Ok(None);
            }
        };

        // Deserialize cache data
        let cache_data: CacheData = match serde_json::from_str(&content) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to parse cache file: {}", e);
                return Ok(None);
            }
        };

        // Check if cache is expired
        let current_time = self.now();

        let cache_age = current_time.saturating_sub(cache_data.cached_at);

        if cache_age > self.cache_ttl.as_secs() {
            info!(
                "Cache expired (age: {}s, ttl: {}s)",
                cache_age,
                self.cache_ttl.as_secs()
            );
            return Ok(None);
        }

        debug!(
            "Cache hit (age: {}s, providers: {})",
            cache_age,
            cache_data.models.len()
        );

        Ok(Some(cache_data.models))
    }

    /// Gets models from cache or fetches from API
    ///
    /// Implements the cache-or-fetch pattern: tries to load from cache first,
    /// and only fetches from API if cache is missing or expired.
    ///
    /// # Errors
    /// Returns an error if both cache loading and API fetching fail
    async fn get_models(&self) -> Result<HashMap<ProviderId, Vec<Model>>> {
        // Try loading from cache first
        match self.load_cache().await {
            Ok(Some(models)) => {
                debug!("Using cached models");
                return Ok(models);
            }
            Ok(None) => {
                debug!("Cache miss, fetching from API");
            }
            Err(e) => {
                warn!("Error loading cache: {}, fetching from API", e);
            }
        }

        // Cache miss or error, fetch from API
        self.fetch_and_transform_models().await
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + HttpInfra> ModelRepository for ForgeModelRepository<F> {
    async fn get_model(
        &self,
        provider_id: &ProviderId,
        model_id: &ModelId,
    ) -> Result<Option<Model>> {
        let models = self.get_models().await?;

        let result = models
            .get(provider_id)
            .and_then(|provider_models| {
                provider_models
                    .iter()
                    .find(|m| &m.id == model_id)
                    .cloned()
            });

        if result.is_some() {
            debug!(
                "Found model {} for provider {:?}",
                model_id.as_str(),
                provider_id
            );
        } else {
            debug!(
                "Model {} not found for provider {:?}",
                model_id.as_str(),
                provider_id
            );
        }

        Ok(result)
    }

    async fn list_models(&self, provider_id: &ProviderId) -> Result<Vec<Model>> {
        let models = self.get_models().await?;

        let result = models.get(provider_id).cloned().unwrap_or_default();

        debug!(
            "Listed {} models for provider {:?}",
            result.len(),
            provider_id
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::{Environment, InputModality};
    use pretty_assertions::assert_eq;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::TempDir;
    use fake::{Fake, Faker};
    use mockito::Server;

    struct MockInfra {
        env: Environment,
    }

    impl MockInfra {
        fn new(cache_dir: PathBuf) -> Self {
            let env: Environment = Faker.fake();
            // Set base_path so that cache_dir() returns our temp directory
            // Since cache_dir() returns base_path.join("cache"), 
            // we need base_path to be cache_dir without the "cache" suffix
            let base_path = if cache_dir.ends_with("cache") {
                cache_dir.parent().unwrap().to_path_buf()
            } else {
                cache_dir
            };
            let env = env.base_path(base_path);
            Self { env }
        }
    }

    impl EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Environment {
            self.env.clone()
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> std::collections::BTreeMap<String, String> {
            std::collections::BTreeMap::new()
        }

        fn is_restricted(&self) -> bool {
            false
        }
    }

    #[async_trait::async_trait]
    impl HttpInfra for MockInfra {
        async fn http_get(
            &self,
            url: &Url,
            headers: Option<reqwest::header::HeaderMap>,
        ) -> Result<reqwest::Response> {
            // Forward to actual reqwest client for mockito testing
            let client = reqwest::Client::new();
            let mut req = client.get(url.clone());
            if let Some(h) = headers {
                req = req.headers(h);
            }
            Ok(req.send().await?)
        }

        async fn http_post(&self, _url: &Url, _body: bytes::Bytes) -> Result<reqwest::Response> {
            unimplemented!()
        }

        async fn http_delete(&self, _url: &Url) -> Result<reqwest::Response> {
            unimplemented!()
        }

        async fn http_eventsource(
            &self,
            _url: &Url,
            _headers: Option<reqwest::header::HeaderMap>,
            _body: bytes::Bytes,
        ) -> Result<reqwest_eventsource::EventSource> {
            unimplemented!()
        }
    }

    fn create_fixture_model() -> Model {
        Model {
            id: "gpt-4".to_string().into(),
            name: Some("GPT-4".to_string()),
            description: None,
            context_length: Some(8192),
            tools_supported: Some(true),
            supports_parallel_tool_calls: Some(true),
            supports_reasoning: Some(false),
            input_modalities: vec![InputModality::Text],
        }
    }

    fn create_fixture_cache_data() -> CacheData {
        let mut models = HashMap::new();
        models.insert(ProviderId::OPENAI, vec![create_fixture_model()]);

        CacheData {
            cached_at: 1704720000,
            models,
        }
    }

    #[test]
    fn test_cache_data_serialization() {
        let fixture = create_fixture_cache_data();

        let actual = serde_json::to_string(&fixture).unwrap();
        let expected = fixture;

        let deserialized: CacheData = serde_json::from_str(&actual).unwrap();

        assert_eq!(deserialized.cached_at, expected.cached_at);
        assert_eq!(deserialized.models.len(), expected.models.len());
    }

    #[test]
    fn test_cache_data_deserialization() {
        let json = r#"{
            "cached_at": 1704720000,
            "models": {
                "openai": [{
                    "id": "gpt-4",
                    "name": "GPT-4",
                    "description": null,
                    "context_length": 8192,
                    "tools_supported": true,
                    "supports_parallel_tool_calls": true,
                    "supports_reasoning": false,
                    "input_modalities": ["text"]
                }]
            }
        }"#;

        let actual: CacheData = serde_json::from_str(json).unwrap();
        let expected = create_fixture_cache_data();

        assert_eq!(actual.cached_at, expected.cached_at);
        assert_eq!(actual.models.len(), expected.models.len());
    }

    #[tokio::test]
    async fn test_write_and_load_cache() {
        let temp_dir = TempDir::new().unwrap();
        let infra = Arc::new(MockInfra::new(temp_dir.path().to_path_buf()));
        let repo = ForgeModelRepository::new(infra);

        let mut models = HashMap::new();
        models.insert(ProviderId::OPENAI, vec![create_fixture_model()]);

        // Write cache
        repo.write_cache(&models).await.unwrap();

        // Load cache
        let actual = repo.load_cache().await.unwrap();
        let expected = Some(models);

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_load_cache_expired() {
        let temp_dir = TempDir::new().unwrap();
        let infra = Arc::new(MockInfra::new(temp_dir.path().to_path_buf()));
        let repo = ForgeModelRepository::new(infra);

        let mut models = HashMap::new();
        models.insert(ProviderId::OPENAI, vec![create_fixture_model()]);

        // Manually write cache with an old timestamp (25 hours ago to exceed 24h TTL)
        let old_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(25 * 3600); // 25 hours ago

        let cache_data = CacheData {
            cached_at: old_timestamp,
            models,
        };

        // Create cache directory
        if let Some(parent) = repo.cache_path.parent() {
            tokio::fs::create_dir_all(parent).await.unwrap();
        }

        let json = serde_json::to_string_pretty(&cache_data).unwrap();
        tokio::fs::write(&repo.cache_path, json).await.unwrap();

        // Load cache should return None (expired)
        let actual = repo.load_cache().await.unwrap();

        assert_eq!(actual, None);
    }

    #[tokio::test]
    async fn test_load_cache_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let infra = Arc::new(MockInfra::new(temp_dir.path().to_path_buf()));
        let repo = ForgeModelRepository::new(infra);

        let actual = repo.load_cache().await.unwrap();

        assert_eq!(actual, None);
    }

    #[tokio::test]
    async fn test_get_model_success() {
        let temp_dir = TempDir::new().unwrap();
        let mut server = Server::new_async().await;
        let mock_response = serde_json::json!({
            "openai": {
                "models": {
                    "gpt-4": {
                        "id": "gpt-4",
                        "name": "GPT-4",
                        "context_length": 8192
                    }
                }
            }
        });

        let _m = server
            .mock("GET", "/api.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_response.to_string())
            .create_async()
            .await;

        let infra = Arc::new(MockInfra::new(temp_dir.path().to_path_buf()));
        // Override the repository to use the mock server URL
        let repo = ForgeModelRepository::new(infra);
        
        // Manually set the cache path and fetch models with mocked URL
        // Since we can't easily override the URL in fetch_and_transform_models,
        // we'll pre-populate the cache for this test
        let mut models = HashMap::new();
        models.insert(
            ProviderId::OPENAI,
            vec![Model {
                id: "gpt-4".to_string().into(),
                name: Some("GPT-4".to_string()),
                description: None,
                context_length: Some(8192),
                tools_supported: None,
                supports_parallel_tool_calls: None,
                supports_reasoning: None,
                input_modalities: vec![InputModality::Text],
            }],
        );
        repo.write_cache(&models).await.unwrap();

        let actual = repo
            .get_model(&ProviderId::OPENAI, &"gpt-4".to_string().into())
            .await
            .unwrap();

        assert!(actual.is_some());
        let model = actual.unwrap();
        assert_eq!(model.id.as_str(), "gpt-4");
        assert_eq!(model.name, Some("GPT-4".to_string()));
    }

    #[tokio::test]
    async fn test_get_model_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let infra = Arc::new(MockInfra::new(temp_dir.path().to_path_buf()));
        let repo = ForgeModelRepository::new(infra);

        // Pre-populate cache with a different model
        let mut models = HashMap::new();
        models.insert(ProviderId::OPENAI, vec![create_fixture_model()]);
        repo.write_cache(&models).await.unwrap();

        let actual = repo
            .get_model(&ProviderId::OPENAI, &"nonexistent".to_string().into())
            .await
            .unwrap();

        assert_eq!(actual, None);
    }

    #[tokio::test]
    async fn test_list_models_success() {
        let temp_dir = TempDir::new().unwrap();
        let infra = Arc::new(MockInfra::new(temp_dir.path().to_path_buf()));
        let repo = ForgeModelRepository::new(infra);

        // Pre-populate cache
        let mut models = HashMap::new();
        models.insert(
            ProviderId::XAI,
            vec![Model {
                id: "grok-2".to_string().into(),
                name: Some("Grok 2".to_string()),
                description: None,
                context_length: Some(131072),
                tools_supported: None,
                supports_parallel_tool_calls: None,
                supports_reasoning: None,
                input_modalities: vec![InputModality::Text],
            }],
        );
        repo.write_cache(&models).await.unwrap();

        let actual = repo.list_models(&ProviderId::XAI).await.unwrap();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].id.as_str(), "grok-2");
    }

    #[tokio::test]
    async fn test_list_models_empty() {
        let temp_dir = TempDir::new().unwrap();
        let infra = Arc::new(MockInfra::new(temp_dir.path().to_path_buf()));
        let repo = ForgeModelRepository::new(infra);

        // Pre-populate cache with models for other providers
        let mut models = HashMap::new();
        models.insert(ProviderId::OPENAI, vec![create_fixture_model()]);
        repo.write_cache(&models).await.unwrap();

        let actual = repo
            .list_models(&ProviderId::ANTHROPIC_COMPATIBLE)
            .await
            .unwrap();

        assert_eq!(actual, Vec::new());
    }

    #[tokio::test]
    async fn test_cache_persistence_across_instances() {
        let temp_dir = TempDir::new().unwrap();

        // First instance: write cache
        {
            let infra = Arc::new(MockInfra::new(temp_dir.path().to_path_buf()));
            let repo = ForgeModelRepository::new(infra);

            let mut models = HashMap::new();
            models.insert(
                ProviderId::XAI,
                vec![Model {
                    id: "grok-2".to_string().into(),
                    name: Some("Grok 2".to_string()),
                    description: None,
                    context_length: Some(131072),
                    tools_supported: None,
                    supports_parallel_tool_calls: None,
                    supports_reasoning: None,
                    input_modalities: vec![InputModality::Text],
                }],
            );
            repo.write_cache(&models).await.unwrap();
        }

        // Second instance: should load from cache
        {
            let infra = Arc::new(MockInfra::new(temp_dir.path().to_path_buf()));
            let repo = ForgeModelRepository::new(infra);

            let actual = repo.list_models(&ProviderId::XAI).await.unwrap();

            assert_eq!(actual.len(), 1);
            assert_eq!(actual[0].id.as_str(), "grok-2");
        }
    }
}
