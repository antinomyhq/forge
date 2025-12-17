use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use bytes::Bytes;
use forge_app::domain::{ProviderId, ProviderResponse};
use forge_app::{EnvironmentInfra, FileReaderInfra, FileWriterInfra};
use forge_domain::{
    AnyProvider, ApiKey, AuthCredential, AuthDetails, Error, MigrationResult, Provider,
    ProviderRepository, ProviderType, URLParam, URLParamValue,
};
use handlebars::Handlebars;
use merge::Merge;
use serde::Deserialize;
use serde_with::{serde_as, VecSkipError};
use url::Url;

/// Error inspector that logs warnings for skipped credential entries.
struct CredentialErrorLogger;

impl serde_with::InspectError for CredentialErrorLogger {
    fn inspect_error(error: impl serde::de::Error) {
        tracing::warn!(
            "Skipping corrupt credential entry in .credentials.json: {}",
            error
        );
    }
}

/// Wrapper for deserializing credentials with error tolerance.
///
/// Uses `VecSkipError` to skip any corrupt credential entries instead of
/// failing the entire deserialization. This ensures Forge can boot up even
/// with a partially corrupt `.credentials.json` file.
#[serde_as]
#[derive(Deserialize)]
struct ResilientCredentials(
    #[serde_as(as = "VecSkipError<_, CredentialErrorLogger>")] Vec<AuthCredential>,
);

/// Represents the source of models for a provider
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Models {
    /// Models are fetched from a URL
    Url(String),
    /// Models are hardcoded in the configuration
    Hardcoded(Vec<forge_app::domain::Model>),
}

#[derive(Debug, Clone, Deserialize, Merge)]
struct ProviderConfig {
    #[merge(strategy = overwrite)]
    id: ProviderId,
    #[serde(default)]
    #[merge(strategy = overwrite)]
    provider_type: ProviderType,
    #[serde(default)]
    #[merge(strategy = overwrite)]
    api_key_vars: Option<String>,
    #[serde(default)]
    #[merge(strategy = merge::vec::append)]
    url_param_vars: Vec<String>,
    #[serde(default)]
    #[merge(strategy = overwrite)]
    response_type: Option<ProviderResponse>,
    #[merge(strategy = overwrite)]
    url: String,
    #[serde(default)]
    #[merge(strategy = overwrite)]
    models: Option<Models>,
    #[merge(strategy = merge::vec::append)]
    auth_methods: Vec<forge_domain::AuthMethod>,
}

fn overwrite<T>(base: &mut T, other: T) {
    *base = other;
}

/// Transparent wrapper for Vec<ProviderConfig> that implements custom merge
/// logic
#[derive(Debug, Clone, Deserialize, Merge)]
#[serde(transparent)]
struct ProviderConfigs(#[merge(strategy = merge_configs)] Vec<ProviderConfig>);

fn merge_configs(base: &mut Vec<ProviderConfig>, other: Vec<ProviderConfig>) {
    let mut map: std::collections::HashMap<_, _> =
        base.drain(..).map(|c| (c.id.clone(), c)).collect();

    for other_config in other {
        let id = other_config.id.clone();
        map.entry(id)
            .and_modify(|base_config| base_config.merge(other_config.clone()))
            .or_insert(other_config);
    }

    base.extend(map.into_values());
}

impl From<&ProviderConfig>
    for Provider<
        forge_domain::Template<HashMap<forge_domain::URLParam, forge_domain::URLParamValue>>,
    >
{
    fn from(config: &ProviderConfig) -> Self {
        let models = config.models.as_ref().map(|m| match m {
            Models::Url(model_url_template) => {
                forge_domain::ModelSource::Url(forge_domain::Template::new(model_url_template))
            }
            Models::Hardcoded(model_list) => {
                forge_domain::ModelSource::Hardcoded(model_list.clone())
            }
        });

        Provider {
            id: config.id.clone(),
            provider_type: config.provider_type,
            response: config.response_type.clone(),
            url: forge_domain::Template::new(&config.url),
            auth_methods: config.auth_methods.clone(),
            url_params: config
                .url_param_vars
                .iter()
                .map(|v| URLParam::from(v.clone()))
                .collect(),
            credential: None,
            models,
        }
    }
}

static HANDLEBARS: OnceLock<Handlebars<'static>> = OnceLock::new();
static PROVIDER_CONFIGS: OnceLock<Vec<ProviderConfig>> = OnceLock::new();

fn get_handlebars() -> &'static Handlebars<'static> {
    HANDLEBARS.get_or_init(Handlebars::new)
}

fn get_provider_configs() -> &'static Vec<ProviderConfig> {
    PROVIDER_CONFIGS.get_or_init(|| {
        let json_str = include_str!("provider.json");
        serde_json::from_str(json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse embedded provider configs: {e}"))
            .unwrap()
    })
}

pub struct ForgeProviderRepository<F> {
    infra: Arc<F>,
    handlebars: &'static Handlebars<'static>,
}

impl<F> ForgeProviderRepository<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, handlebars: get_handlebars() }
    }
}

impl<F: EnvironmentInfra + FileReaderInfra + FileWriterInfra> ForgeProviderRepository<F> {
    async fn get_custom_provider_configs(&self) -> anyhow::Result<Vec<ProviderConfig>> {
        let environment = self.infra.get_environment();
        let provider_json_path = environment.base_path.join("provider.json");

        let json_str = self.infra.read_utf8(&provider_json_path).await?;
        let configs = serde_json::from_str(&json_str)?;
        Ok(configs)
    }

    async fn get_providers(&self) -> Vec<AnyProvider> {
        let configs = self.get_merged_configs().await;

        let mut providers: Vec<AnyProvider> = Vec::new();
        for config in configs {
            // Skip Forge provider as it's handled specially
            if config.id == ProviderId::FORGE {
                continue;
            }

            // Try to create configured provider, fallback to unconfigured
            let provider_entry = if let Ok(provider) = self.create_provider(&config).await {
                Some(provider.into())
            } else if let Ok(provider) = self.create_unconfigured_provider(&config) {
                Some(provider.into())
            } else {
                None
            };

            if let Some(entry) = provider_entry {
                providers.push(entry);
            }
        }

        // Sort by ProviderId enum order to ensure deterministic, priority-based
        // ordering
        providers.sort_by_key(|a| a.id());

        providers
    }

    /// Migrates environment variable-based credentials to file-based
    /// credentials. This is a one-time migration that runs only if the
    /// credentials file doesn't exist.
    pub async fn migrate_env_to_file(&self) -> anyhow::Result<Option<MigrationResult>> {
        let path = self
            .infra
            .get_environment()
            .base_path
            .join(".credentials.json");

        // Check if credentials file already exists
        if self.infra.read_utf8(&path).await.is_ok() {
            return Ok(None);
        }

        let mut credentials = Vec::new();
        let mut migrated_providers = Vec::new();
        let configs = self.get_merged_configs().await;

        let has_openai_url = self.infra.get_env_var("OPENAI_URL").is_some();
        let has_anthropic_url = self.infra.get_env_var("ANTHROPIC_URL").is_some();

        for config in configs {
            // Skip Forge provider and ContextEngine providers - they're not configurable
            // via env like other providers
            if config.id == ProviderId::FORGE || config.provider_type == ProviderType::ContextEngine
            {
                continue;
            }

            if config.id == ProviderId::OPENAI && has_openai_url {
                continue;
            }
            if config.id == ProviderId::OPENAI_COMPATIBLE && !has_openai_url {
                continue;
            }
            if config.id == ProviderId::ANTHROPIC && has_anthropic_url {
                continue;
            }
            if config.id == ProviderId::ANTHROPIC_COMPATIBLE && !has_anthropic_url {
                continue;
            }

            // Try to create credential from environment variables
            if let Ok(credential) = self.create_credential_from_env(&config) {
                migrated_providers.push(config.id);
                credentials.push(credential);
            }
        }

        // Only write if we have credentials to migrate
        if !credentials.is_empty() {
            self.write_credentials(&credentials).await?;
            Ok(Some(MigrationResult::new(path, migrated_providers)))
        } else {
            Ok(None)
        }
    }

    /// Creates a credential from environment variables for a given config
    fn create_credential_from_env(
        &self,
        config: &ProviderConfig,
    ) -> anyhow::Result<AuthCredential> {
        // Check API key environment variable (if specified)
        let api_key = if let Some(api_key_var) = &config.api_key_vars {
            self.infra
                .get_env_var(api_key_var)
                .ok_or_else(|| Error::env_var_not_found(config.id.clone(), api_key_var))?
        } else {
            // For context engine, we don't use env vars for API key
            String::new()
        };

        // Check URL parameter environment variables
        let mut url_params = std::collections::HashMap::new();

        for env_var in &config.url_param_vars {
            if let Some(value) = self.infra.get_env_var(env_var) {
                url_params.insert(URLParam::from(env_var.clone()), URLParamValue::from(value));
            } else {
                return Err(Error::env_var_not_found(config.id.clone(), env_var).into());
            }
        }

        // Create AuthCredential
        Ok(AuthCredential {
            id: config.id.clone(),
            auth_details: AuthDetails::ApiKey(ApiKey::from(api_key)),
            url_params,
        })
    }

    /// Creates a configured provider from file-based credentials.
    /// The credential file (.credentials.json) is the single source of
    /// truth.
    async fn create_provider(&self, config: &ProviderConfig) -> anyhow::Result<Provider<Url>> {
        // Get credential from file
        let credential = self
            .get_credential(&config.id)
            .await?
            .ok_or_else(|| Error::provider_not_available(config.id.clone()))?;

        // Build template data from URL parameters in credential
        let mut template_data = std::collections::HashMap::new();
        for (param, value) in &credential.url_params {
            template_data.insert(param.as_str(), value.as_str());
        }

        // Render URL using handlebars
        let url = self
            .handlebars
            .render_template(&config.url, &template_data)
            .map_err(|e| {
                anyhow::anyhow!("Failed to render URL template for {}: {}", config.id, e)
            })?;
        let final_url = Url::parse(&url)?;

        // Handle models based on the variant
        let models = config.models.as_ref().map(|m| match m {
            Models::Url(model_url_template) => {
                let model_url = Url::parse(
                    &self
                        .handlebars
                        .render_template(model_url_template, &template_data)
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to render model_url template for {}: {}",
                                config.id,
                                e
                            )
                        })
                        .unwrap(),
                )
                .unwrap();
                forge_domain::ModelSource::Url(model_url)
            }
            Models::Hardcoded(model_list) => {
                forge_domain::ModelSource::Hardcoded(model_list.clone())
            }
        });

        Ok(Provider {
            id: config.id.clone(),
            provider_type: config.provider_type,
            response: config.response_type.clone(),
            url: final_url,
            auth_methods: config.auth_methods.clone(),
            url_params: config
                .url_param_vars
                .iter()
                .map(|v| URLParam::from(v.clone()))
                .collect(),
            credential: Some(credential),
            models,
        })
    }

    /// Creates an unconfigured provider when environment variables are missing.
    fn create_unconfigured_provider(
        &self,
        config: &ProviderConfig,
    ) -> anyhow::Result<
        Provider<
            forge_domain::Template<HashMap<forge_domain::URLParam, forge_domain::URLParamValue>>,
        >,
    > {
        Ok(config.into())
    }

    async fn provider_from_id(&self, id: ProviderId) -> anyhow::Result<Provider<Url>> {
        // Handle special cases first
        if id == ProviderId::FORGE {
            // Forge provider isn't typically configured via env vars in the registry
            return Err(Error::provider_not_available(ProviderId::FORGE).into());
        }

        // Look up provider from cached providers - only return configured ones
        self.get_providers()
            .await
            .iter()
            .find_map(|p| match p {
                AnyProvider::Url(cp) if cp.id == id => Some(cp.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::provider_not_available(id).into())
    }

    /// Returns merged provider configs (embedded + custom)
    async fn get_merged_configs(&self) -> Vec<ProviderConfig> {
        let mut configs = ProviderConfigs(get_provider_configs().clone());
        // Merge custom configs into embedded configs
        configs.merge(ProviderConfigs(
            self.get_custom_provider_configs().await.unwrap_or_default(),
        ));

        configs.0
    }

    /// Reads credentials from the JSON file with multiple fallback strategies:
    /// 1. Try normal JSON parsing with `VecSkipError` to skip corrupt entries
    /// 2. If JSON is syntactically broken, try to repair it with
    ///    `forge_json_repair`
    /// 3. If all else fails, return an empty Vec
    async fn read_credentials(&self) -> Vec<AuthCredential> {
        let path = self
            .infra
            .get_environment()
            .base_path
            .join(".credentials.json");

        let Ok(content) = self.infra.read_utf8(&path).await else {
            return Vec::new();
        };

        forge_json_repair::from_str::<ResilientCredentials>(&content)
            .map(|r| r.0)
            .unwrap_or_default()
    }

    /// Writes credentials to the JSON file
    async fn write_credentials(&self, credentials: &Vec<AuthCredential>) -> anyhow::Result<()> {
        let path = self
            .infra
            .get_environment()
            .base_path
            .join(".credentials.json");

        let content = serde_json::to_string_pretty(credentials)?;
        self.infra.write(&path, Bytes::from(content)).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + FileReaderInfra + FileWriterInfra + Sync> ProviderRepository
    for ForgeProviderRepository<F>
{
    async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>> {
        Ok(self.get_providers().await.clone())
    }

    async fn get_provider(&self, id: ProviderId) -> anyhow::Result<Provider<Url>> {
        self.provider_from_id(id).await
    }

    async fn upsert_credential(&self, credential: AuthCredential) -> anyhow::Result<()> {
        let mut credentials = self.read_credentials().await;
        let id = credential.id.clone();
        // Update existing credential or add new one
        if let Some(existing) = credentials.iter_mut().find(|c| c.id == id) {
            *existing = credential;
        } else {
            credentials.push(credential);
        }
        self.write_credentials(&credentials).await?;

        Ok(())
    }

    async fn get_credential(&self, id: &ProviderId) -> anyhow::Result<Option<AuthCredential>> {
        let credentials = self.read_credentials().await;
        Ok(credentials.into_iter().find(|c| &c.id == id))
    }

    async fn remove_credential(&self, id: &ProviderId) -> anyhow::Result<()> {
        let mut credentials = self.read_credentials().await;
        credentials.retain(|c| &c.id != id);
        self.write_credentials(&credentials).await?;

        Ok(())
    }

    async fn migrate_env_credentials(&self) -> anyhow::Result<Option<MigrationResult>> {
        self.migrate_env_to_file().await
    }
}

#[cfg(test)]
mod tests {
    use forge_app::domain::ProviderResponse;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_load_provider_configs() {
        let configs = get_provider_configs();
        assert!(!configs.is_empty());

        // Test that OpenRouter config is loaded correctly
        let openrouter_config = configs
            .iter()
            .find(|c| c.id == ProviderId::OPEN_ROUTER)
            .unwrap();
        assert_eq!(
            openrouter_config.api_key_vars,
            Some("OPENROUTER_API_KEY".to_string())
        );
        assert_eq!(openrouter_config.url_param_vars, Vec::<String>::new());
        assert_eq!(
            openrouter_config.response_type,
            Some(ProviderResponse::OpenAI)
        );
        assert_eq!(
            openrouter_config.url.as_str(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[test]
    fn test_vertex_ai_config() {
        let configs = get_provider_configs();
        let config = configs
            .iter()
            .find(|c| c.id == ProviderId::VERTEX_AI)
            .unwrap();
        assert_eq!(config.id, ProviderId::VERTEX_AI);
        assert_eq!(
            config.api_key_vars,
            Some("VERTEX_AI_AUTH_TOKEN".to_string())
        );
        assert_eq!(
            config.url_param_vars,
            vec!["PROJECT_ID".to_string(), "LOCATION".to_string()]
        );
        assert_eq!(config.response_type, Some(ProviderResponse::OpenAI));
        assert!(&config.url.contains("{{"));
        assert!(&config.url.contains("}}"));
    }

    #[test]
    fn test_azure_config() {
        let configs = get_provider_configs();
        let config = configs.iter().find(|c| c.id == ProviderId::AZURE).unwrap();
        assert_eq!(config.id, ProviderId::AZURE);
        assert_eq!(config.api_key_vars, Some("AZURE_API_KEY".to_string()));
        assert_eq!(
            config.url_param_vars,
            vec![
                "AZURE_RESOURCE_NAME".to_string(),
                "AZURE_DEPLOYMENT_NAME".to_string(),
                "AZURE_API_VERSION".to_string()
            ]
        );
        assert_eq!(config.response_type, Some(ProviderResponse::OpenAI));

        // Check URL (now contains full chat completion URL)
        let url = &config.url;
        assert!(url.contains("{{"));
        assert!(url.contains("}}"));
        assert!(url.contains("openai.azure.com"));
        assert!(url.contains("api-version"));
        assert!(url.contains("deployments"));
        assert!(url.contains("chat/completions"));

        // Check models exists and contains expected elements
        match config.models.as_ref().unwrap() {
            Models::Url(model_url) => {
                assert!(model_url.contains("api-version"));
                assert!(model_url.contains("/models"));
            }
            Models::Hardcoded(_) => panic!("Expected Models::Url variant"),
        }
    }

    #[test]
    fn test_openai_compatible_config() {
        let configs = get_provider_configs();
        let config = configs
            .iter()
            .find(|c| c.id == ProviderId::OPENAI_COMPATIBLE)
            .unwrap();
        assert_eq!(config.id, ProviderId::OPENAI_COMPATIBLE);
        assert_eq!(config.api_key_vars, Some("OPENAI_API_KEY".to_string()));
        assert_eq!(config.url_param_vars, vec!["OPENAI_URL".to_string()]);
        assert_eq!(config.response_type, Some(ProviderResponse::OpenAI));
        assert!(&config.url.contains("{{OPENAI_URL}}"));
    }

    #[test]
    fn test_anthropic_compatible_config() {
        let configs = get_provider_configs();
        let config = configs
            .iter()
            .find(|c| c.id == ProviderId::ANTHROPIC_COMPATIBLE)
            .unwrap();
        assert_eq!(config.id, ProviderId::ANTHROPIC_COMPATIBLE);
        assert_eq!(config.api_key_vars, Some("ANTHROPIC_API_KEY".to_string()));
        assert_eq!(config.url_param_vars, vec!["ANTHROPIC_URL".to_string()]);
        assert_eq!(config.response_type, Some(ProviderResponse::Anthropic));
        assert!(config.url.contains("{{ANTHROPIC_URL}}"));
    }

    #[test]
    fn test_io_intelligence_config() {
        let configs = get_provider_configs();
        let config = configs
            .iter()
            .find(|c| c.id == ProviderId::IO_INTELLIGENCE)
            .unwrap();
        assert_eq!(config.id, ProviderId::IO_INTELLIGENCE);
        assert_eq!(
            config.api_key_vars,
            Some("IO_INTELLIGENCE_API_KEY".to_string())
        );
        assert_eq!(config.url_param_vars, Vec::<String>::new());
        assert_eq!(config.response_type, Some(ProviderResponse::OpenAI));
        assert_eq!(
            config.url.as_str(),
            "https://api.intelligence.io.solutions/api/v1/chat/completions"
        );
    }
}

#[cfg(test)]
mod env_tests {
    use std::collections::{BTreeMap, HashMap};
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_app::domain::Environment;
    use forge_domain::AnyProvider;
    use pretty_assertions::assert_eq;

    use super::*;

    // Mock infrastructure that provides environment variables
    struct MockInfra {
        env_vars: HashMap<String, String>,
        base_path: PathBuf,
        credentials_json: std::sync::RwLock<Option<String>>,
    }

    impl Default for MockInfra {
        fn default() -> Self {
            use fake::{Fake, Faker};
            Self {
                env_vars: HashMap::new(),
                base_path: Faker.fake(),
                credentials_json: std::sync::RwLock::new(None),
            }
        }
    }

    impl MockInfra {
        fn new(env_vars: HashMap<String, String>) -> Self {
            Self { env_vars, ..Default::default() }
        }

        fn credentials_json(self, json: impl Into<String>) -> Self {
            *self.credentials_json.write().unwrap() = Some(json.into());
            self
        }
    }

    impl EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            let mut env: Environment = Faker.fake();
            env.base_path = self.base_path.clone();
            env
        }

        fn get_env_var(&self, key: &str) -> Option<String> {
            self.env_vars.get(key).cloned()
        }

        fn get_env_vars(&self) -> BTreeMap<String, String> {
            self.env_vars
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        }
    }

    #[async_trait::async_trait]
    impl FileReaderInfra for MockInfra {
        async fn read_utf8(&self, path: &std::path::Path) -> anyhow::Result<String> {
            if path.ends_with(".credentials.json") {
                let guard = self.credentials_json.read().unwrap();
                if let Some(ref json) = *guard {
                    return Ok(json.clone());
                }
            }
            Err(anyhow::anyhow!("File not found"))
        }

        async fn read(&self, _path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
            Err(anyhow::anyhow!("File not found"))
        }

        async fn range_read_utf8(
            &self,
            _path: &std::path::Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, forge_domain::FileInfo)> {
            Err(anyhow::anyhow!("File not found"))
        }
    }

    #[async_trait::async_trait]
    impl FileWriterInfra for MockInfra {
        async fn write(&self, path: &std::path::Path, content: Bytes) -> anyhow::Result<()> {
            if path.ends_with(".credentials.json") {
                let mut guard = self.credentials_json.write().unwrap();
                *guard = Some(String::from_utf8(content.to_vec())?);
            }
            Ok(())
        }

        async fn write_temp(
            &self,
            _prefix: &str,
            _ext: &str,
            _content: &str,
        ) -> anyhow::Result<PathBuf> {
            Ok(PathBuf::from("/tmp/test"))
        }
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockInfra {
        async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>> {
            Ok(vec![])
        }

        async fn get_provider(&self, _id: ProviderId) -> anyhow::Result<Provider<Url>> {
            Err(anyhow::anyhow!("Provider not found"))
        }

        async fn upsert_credential(
            &self,
            _credential: forge_domain::AuthCredential,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_credential(
            &self,
            _id: &ProviderId,
        ) -> anyhow::Result<Option<forge_domain::AuthCredential>> {
            Ok(None)
        }

        async fn remove_credential(&self, _id: &ProviderId) -> anyhow::Result<()> {
            Ok(())
        }

        async fn migrate_env_credentials(
            &self,
        ) -> anyhow::Result<Option<forge_domain::MigrationResult>> {
            Ok(None)
        }
    }

    impl MockInfra {
        fn get_credentials(&self) -> Vec<AuthCredential> {
            let guard = self.credentials_json.read().unwrap();
            guard
                .as_ref()
                .and_then(|json| serde_json::from_str(json).ok())
                .unwrap_or_default()
        }
    }

    #[tokio::test]
    async fn test_migration_from_env_to_file() {
        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-openai-key".to_string());
        env_vars.insert(
            "ANTHROPIC_API_KEY".to_string(),
            "test-anthropic-key".to_string(),
        );
        env_vars.insert(
            "OPENAI_URL".to_string(),
            "https://custom.openai.com/v1".to_string(),
        );

        let infra = Arc::new(MockInfra::new(env_vars));
        let registry = ForgeProviderRepository::new(infra.clone());

        // Trigger migration
        registry.migrate_env_to_file().await.unwrap();

        // Verify credentials were written
        let credentials = infra.get_credentials();

        // Should have migrated OpenAICompatible (not OpenAI) and Anthropic (not
        // AnthropicCompatible)
        assert!(
            !credentials.iter().any(|c| c.id == ProviderId::OPENAI),
            "Should NOT create OpenAI credential when OPENAI_URL is set"
        );
        assert!(
            credentials
                .iter()
                .any(|c| c.id == ProviderId::OPENAI_COMPATIBLE),
            "Should create OpenAICompatible credential when OPENAI_URL is set"
        );
        assert!(
            credentials.iter().any(|c| c.id == ProviderId::ANTHROPIC),
            "Should create Anthropic credential when ANTHROPIC_URL is NOT set"
        );
        assert!(
            !credentials
                .iter()
                .any(|c| c.id == ProviderId::ANTHROPIC_COMPATIBLE),
            "Should NOT create AnthropicCompatible credential when ANTHROPIC_URL is NOT set"
        );

        // Verify OpenAICompatible credential
        let openai_compat_cred = credentials
            .iter()
            .find(|c| c.id == ProviderId::OPENAI_COMPATIBLE)
            .unwrap();
        match &openai_compat_cred.auth_details {
            AuthDetails::ApiKey(key) => assert_eq!(key.as_str(), "test-openai-key"),
            _ => panic!("Expected API key"),
        }

        // Verify OpenAICompatible has URL param
        assert!(!openai_compat_cred.url_params.is_empty());
        let url_params = &openai_compat_cred.url_params;
        assert_eq!(
            url_params
                .get(&URLParam::from("OPENAI_URL".to_string()))
                .map(|v| v.as_str()),
            Some("https://custom.openai.com/v1")
        );

        // Verify Anthropic credential
        let anthropic_cred = credentials
            .iter()
            .find(|c| c.id == ProviderId::ANTHROPIC)
            .unwrap();
        match &anthropic_cred.auth_details {
            AuthDetails::ApiKey(key) => assert_eq!(key.as_str(), "test-anthropic-key"),
            _ => panic!("Expected API key"),
        }
    }

    #[tokio::test]
    async fn test_migration_should_not_create_forge_services_credential() {
        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());

        let infra = Arc::new(MockInfra::new(env_vars));
        let registry = ForgeProviderRepository::new(infra.clone());

        // Trigger migration
        registry.migrate_env_to_file().await.unwrap();

        // Verify credentials were written
        let credentials = infra.get_credentials();

        // Verify forge_services was NOT created during migration
        assert!(
            !credentials
                .iter()
                .any(|c| c.id == ProviderId::FORGE_SERVICES),
            "Should NOT create forge_services credential during environment migration"
        );

        // Verify only OpenAI credential was created
        assert_eq!(
            credentials.len(),
            1,
            "Should only have one credential (OpenAI)"
        );
        assert!(
            credentials.iter().any(|c| c.id == ProviderId::OPENAI),
            "Should have OpenAI credential"
        );
    }

    #[tokio::test]
    async fn test_migration_both_compatible_urls() {
        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-openai-key".to_string());
        env_vars.insert(
            "ANTHROPIC_API_KEY".to_string(),
            "test-anthropic-key".to_string(),
        );
        env_vars.insert(
            "OPENAI_URL".to_string(),
            "https://custom.openai.com/v1".to_string(),
        );
        env_vars.insert(
            "ANTHROPIC_URL".to_string(),
            "https://custom.anthropic.com/v1".to_string(),
        );

        let infra = Arc::new(MockInfra::new(env_vars));
        let registry = ForgeProviderRepository::new(infra.clone());

        // Trigger migration
        registry.migrate_env_to_file().await.unwrap();

        // Verify credentials were written
        let credentials = infra.get_credentials();

        // Should have migrated only compatible versions
        assert!(
            !credentials.iter().any(|c| c.id == ProviderId::OPENAI),
            "Should NOT create OpenAI credential when OPENAI_URL is set"
        );
        assert!(
            credentials
                .iter()
                .any(|c| c.id == ProviderId::OPENAI_COMPATIBLE),
            "Should create OpenAICompatible credential when OPENAI_URL is set"
        );
        assert!(
            !credentials.iter().any(|c| c.id == ProviderId::ANTHROPIC),
            "Should NOT create Anthropic credential when ANTHROPIC_URL is set"
        );
        assert!(
            credentials
                .iter()
                .any(|c| c.id == ProviderId::ANTHROPIC_COMPATIBLE),
            "Should create AnthropicCompatible credential when ANTHROPIC_URL is set"
        );

        // Verify AnthropicCompatible has URL param
        let anthropic_compat_cred = credentials
            .iter()
            .find(|c| c.id == ProviderId::ANTHROPIC_COMPATIBLE)
            .unwrap();
        assert!(!anthropic_compat_cred.url_params.is_empty());
        let url_params = &anthropic_compat_cred.url_params;
        assert_eq!(
            url_params
                .get(&URLParam::from("ANTHROPIC_URL".to_string()))
                .map(|v| v.as_str()),
            Some("https://custom.anthropic.com/v1")
        );
    }

    #[tokio::test]
    async fn test_create_azure_provider_with_handlebars_urls() {
        let mut env_vars = HashMap::new();
        env_vars.insert("AZURE_API_KEY".to_string(), "test-key-123".to_string());
        env_vars.insert(
            "AZURE_RESOURCE_NAME".to_string(),
            "my-test-resource".to_string(),
        );
        env_vars.insert(
            "AZURE_DEPLOYMENT_NAME".to_string(),
            "gpt-4-deployment".to_string(),
        );
        env_vars.insert(
            "AZURE_API_VERSION".to_string(),
            "2024-02-01-preview".to_string(),
        );

        let infra = Arc::new(MockInfra::new(env_vars));
        let registry = ForgeProviderRepository::new(infra);

        // Trigger migration to populate credentials file
        registry.migrate_env_to_file().await.unwrap();

        // Get Azure config from embedded configs
        let configs = get_provider_configs();
        let azure_config = configs
            .iter()
            .find(|c| c.id == ProviderId::AZURE)
            .expect("Azure config should exist");

        // Create provider using the registry's create_provider method
        let provider = registry
            .create_provider(azure_config)
            .await
            .expect("Should create Azure provider");

        // Verify all URLs are correctly rendered
        assert_eq!(provider.id, ProviderId::AZURE);
        assert_eq!(
            provider
                .credential
                .as_ref()
                .and_then(|c| match &c.auth_details {
                    forge_domain::AuthDetails::ApiKey(key) => Some(key.to_string()),
                    _ => None,
                }),
            Some("test-key-123".to_string())
        );

        // Check chat completion URL (url field now contains the chat completion URL)
        let chat_url = provider.url();
        assert_eq!(
            chat_url.as_str(),
            "https://my-test-resource.openai.azure.com/openai/deployments/gpt-4-deployment/chat/completions?api-version=2024-02-01-preview"
        );

        // Check model URL
        match &provider.models.as_ref().unwrap() {
            forge_domain::ModelSource::Url(model_url) => {
                assert_eq!(
                    model_url.as_str(),
                    "https://my-test-resource.openai.azure.com/openai/models?api-version=2024-02-01-preview"
                );
            }
            forge_domain::ModelSource::Hardcoded(_) => panic!("Expected Models::Url variant"),
        }
    }

    #[tokio::test]
    async fn test_default_provider_urls() {
        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());
        env_vars.insert("ANTHROPIC_API_KEY".to_string(), "test-key".to_string());

        let infra = Arc::new(MockInfra::new(env_vars));
        let registry = ForgeProviderRepository::new(infra);

        // Migrate environment variables to .credentials.json
        registry.migrate_env_to_file().await.unwrap();

        let providers = registry.get_all_providers().await.unwrap();

        let openai_provider = providers
            .iter()
            .find_map(|p| match p {
                AnyProvider::Url(cp) if cp.id == ProviderId::OPENAI => Some(cp),
                _ => None,
            })
            .unwrap();
        let anthropic_provider = providers
            .iter()
            .find_map(|p| match p {
                AnyProvider::Url(cp) if cp.id == ProviderId::ANTHROPIC => Some(cp),
                _ => None,
            })
            .unwrap();

        // Regular OpenAI and Anthropic providers use hardcoded URLs
        assert_eq!(
            openai_provider.url.as_str(),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            anthropic_provider.url.as_str(),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[tokio::test]
    async fn test_merge_base_provider_configs() {
        use std::io::Write;

        use tempfile::TempDir;

        // Create a temporary directory to act as base_path
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();

        // Create a custom provider.json in the base directory
        // Only override OpenAI, don't add custom providers
        let provider_json_path = base_path.join("provider.json");
        let mut file = std::fs::File::create(&provider_json_path).unwrap();
        let custom_config = r#"[
            {
                "id": "openai",
                "api_key_vars": "CUSTOM_OPENAI_KEY",
                "url_param_vars": [],
                "response_type": "OpenAI",
                "auth_methods": [],
                "url": "https://custom.openai.com/v1/chat/completions",
                "models": "https://custom.openai.com/v1/models"
            }
        ]"#;
        file.write_all(custom_config.as_bytes()).unwrap();
        drop(file);

        // Create mock infra with the custom base_path
        let mut env_vars = HashMap::new();
        env_vars.insert("CUSTOM_OPENAI_KEY".to_string(), "test-key".to_string());

        struct CustomMockInfra {
            env_vars: HashMap<String, String>,
            base_path: PathBuf,
        }

        impl EnvironmentInfra for CustomMockInfra {
            fn get_environment(&self) -> Environment {
                use fake::{Fake, Faker};
                let mut env: Environment = Faker.fake();
                env.base_path = self.base_path.clone();
                env
            }

            fn get_env_var(&self, key: &str) -> Option<String> {
                self.env_vars.get(key).cloned()
            }

            fn get_env_vars(&self) -> BTreeMap<String, String> {
                self.env_vars
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            }
        }

        #[async_trait::async_trait]
        impl FileReaderInfra for CustomMockInfra {
            async fn read_utf8(&self, path: &std::path::Path) -> anyhow::Result<String> {
                tokio::fs::read_to_string(path).await.map_err(Into::into)
            }

            async fn read(&self, path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
                tokio::fs::read(path).await.map_err(Into::into)
            }

            async fn range_read_utf8(
                &self,
                _path: &std::path::Path,
                _start_line: u64,
                _end_line: u64,
            ) -> anyhow::Result<(String, forge_domain::FileInfo)> {
                Err(anyhow::anyhow!("Not implemented"))
            }
        }

        #[async_trait::async_trait]
        impl FileWriterInfra for CustomMockInfra {
            async fn write(&self, _path: &std::path::Path, _content: Bytes) -> anyhow::Result<()> {
                Ok(())
            }

            async fn write_temp(
                &self,
                _prefix: &str,
                _ext: &str,
                _content: &str,
            ) -> anyhow::Result<PathBuf> {
                Ok(PathBuf::from("/tmp/test"))
            }
        }

        #[async_trait::async_trait]
        impl ProviderRepository for CustomMockInfra {
            async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>> {
                Ok(vec![])
            }

            async fn get_provider(&self, _id: ProviderId) -> anyhow::Result<Provider<Url>> {
                Err(anyhow::anyhow!("Provider not found"))
            }

            async fn upsert_credential(
                &self,
                _credential: forge_domain::AuthCredential,
            ) -> anyhow::Result<()> {
                Ok(())
            }

            async fn get_credential(
                &self,
                _id: &ProviderId,
            ) -> anyhow::Result<Option<forge_domain::AuthCredential>> {
                Ok(None)
            }

            async fn remove_credential(&self, _id: &ProviderId) -> anyhow::Result<()> {
                Ok(())
            }

            async fn migrate_env_credentials(
                &self,
            ) -> anyhow::Result<Option<forge_domain::MigrationResult>> {
                Ok(None)
            }
        }

        let infra = Arc::new(CustomMockInfra { env_vars, base_path });
        let registry = ForgeProviderRepository::new(infra);

        // Get merged configs
        let merged_configs = registry.get_merged_configs().await;

        // Verify OpenAI config was overridden
        let openai_config = merged_configs
            .iter()
            .find(|c| c.id == ProviderId::OPENAI)
            .expect("OpenAI config should exist");
        assert_eq!(
            openai_config.api_key_vars,
            Some("CUSTOM_OPENAI_KEY".to_string())
        );
        assert_eq!(
            openai_config.url.as_str(),
            "https://custom.openai.com/v1/chat/completions"
        );

        // Verify other embedded configs still exist
        let openrouter_config = merged_configs
            .iter()
            .find(|c| c.id == ProviderId::OPEN_ROUTER);
        assert!(openrouter_config.is_some());
    }

    #[tokio::test]
    async fn test_read_credentials_skips_corrupt_entries() {
        let json = r#"[
            {"id": "openai", "auth_details": {"api_key": "valid-key"}},
            {"id": "anthropic"},
            {"id": "open_router", "auth_details": {"api_key": "another-key"}}
        ]"#;

        let infra = Arc::new(MockInfra::default().credentials_json(json));
        let service = ForgeProviderRepository::new(infra);
        let credentials = service.read_credentials().await;

        assert_eq!(credentials.len(), 2);
        assert_eq!(credentials[0].id, ProviderId::OPENAI);
        assert_eq!(credentials[1].id, ProviderId::OPEN_ROUTER);
    }

    #[tokio::test]
    async fn test_read_credentials_skips_invalid_types() {
        let json = r#"[
            {"id": "openai", "auth_details": {"api_key": "key"}},
            "not an object",
            123,
            null,
            {"id": "anthropic", "auth_details": {"api_key": "key2"}}
        ]"#;

        let infra = Arc::new(MockInfra::default().credentials_json(json));
        let service = ForgeProviderRepository::new(infra);
        let credentials = service.read_credentials().await;

        assert_eq!(credentials.len(), 2);
        assert_eq!(credentials[0].id, ProviderId::OPENAI);
        assert_eq!(credentials[1].id, ProviderId::ANTHROPIC);
    }

    #[tokio::test]
    async fn test_read_credentials_all_corrupt_returns_empty() {
        let json = r#"[{"id": "openai"}, {"id": "anthropic"}, "invalid"]"#;

        let infra = Arc::new(MockInfra::default().credentials_json(json));
        let service = ForgeProviderRepository::new(infra);
        let credentials = service.read_credentials().await;

        assert!(credentials.is_empty());
    }

    #[tokio::test]
    async fn test_read_credentials_repairs_trailing_comma() {
        let json = r#"[{"id": "openai", "auth_details": {"api_key": "key"}},]"#;

        let infra = Arc::new(MockInfra::default().credentials_json(json));
        let service = ForgeProviderRepository::new(infra);
        let credentials = service.read_credentials().await;

        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].id, ProviderId::OPENAI);
    }

    #[tokio::test]
    async fn test_read_credentials_repairs_unquoted_keys() {
        let json = r#"[{id: "openai", auth_details: {api_key: "key"}}]"#;

        let infra = Arc::new(MockInfra::default().credentials_json(json));
        let service = ForgeProviderRepository::new(infra);
        let credentials = service.read_credentials().await;

        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].id, ProviderId::OPENAI);
    }

    #[tokio::test]
    async fn test_read_credentials_invalid_json_returns_empty() {
        let json = r#"{"not": "an array"}"#;

        let infra = Arc::new(MockInfra::default().credentials_json(json));
        let service = ForgeProviderRepository::new(infra);
        let credentials = service.read_credentials().await;

        assert!(credentials.is_empty());
    }

    #[tokio::test]
    async fn test_read_credentials_empty_array() {
        let infra = Arc::new(MockInfra::default().credentials_json("[]"));
        let service = ForgeProviderRepository::new(infra);
        let credentials = service.read_credentials().await;

        assert!(credentials.is_empty());
    }

    #[tokio::test]
    async fn test_read_credentials_valid_all_preserved() {
        let json = r#"[
            {"id": "openai", "auth_details": {"api_key": "key1"}},
            {"id": "anthropic", "auth_details": {"api_key": "key2"}}
        ]"#;

        let infra = Arc::new(MockInfra::default().credentials_json(json));
        let service = ForgeProviderRepository::new(infra);
        let credentials = service.read_credentials().await;

        assert_eq!(credentials.len(), 2);
    }
}
