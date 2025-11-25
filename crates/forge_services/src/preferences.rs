use std::sync::Arc;

use forge_app::AppConfigService;
use forge_domain::{
    AppConfig, AppConfigRepository, ModelId, Provider, ProviderId, ProviderRepository,
};
use url::Url;

/// Service for managing user preferences for default providers and models.
pub struct ForgeAppConfigService<F> {
    infra: Arc<F>,
}

impl<F> ForgeAppConfigService<F> {
    /// Creates a new provider preferences service.
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

impl<F: ProviderRepository + AppConfigRepository> ForgeAppConfigService<F> {
    /// Helper method to update app configuration atomically.
    async fn update<U>(&self, updater: U) -> anyhow::Result<()>
    where
        U: FnOnce(&mut AppConfig),
    {
        let mut config = self.infra.get_app_config().await?;
        updater(&mut config);
        self.infra.set_app_config(&config).await?;
        Ok(())
    }

    /// Gets the first available provider from the provider registry.
    async fn get_first_available_provider(&self) -> anyhow::Result<Provider<Url>> {
        self.infra
            .get_all_providers()
            .await?
            .into_iter()
            .find_map(|p| match p {
                forge_domain::AnyProvider::Url(provider) => Some(provider),
                forge_domain::AnyProvider::Template(_) => None,
            })
            .ok_or_else(|| forge_app::Error::NoActiveProvider.into())
    }
}

#[async_trait::async_trait]
impl<F: ProviderRepository + AppConfigRepository + Send + Sync> AppConfigService
    for ForgeAppConfigService<F>
{
    async fn get_default_provider(&self) -> anyhow::Result<Provider<Url>> {
        let app_config = self.infra.get_app_config().await?;
        if let Some(provider_id) = app_config.provider
            && let Ok(provider) = self.infra.get_provider(provider_id).await
            && provider.is_configured()
        {
            return Ok(provider);
        }

        // No active provider set or configured provider not found, try to find the
        // first available one
        self.get_first_available_provider().await
    }

    async fn set_default_provider(&self, provider_id: ProviderId) -> anyhow::Result<()> {
        self.update(|config| {
            config.provider = Some(provider_id);
        })
        .await
    }

    async fn get_provider_model(
        &self,
        provider_id: Option<&ProviderId>,
    ) -> anyhow::Result<ModelId> {
        let config = self.infra.get_app_config().await?;

        let provider_id = match provider_id {
            Some(id) => id,
            None => config
                .provider
                .as_ref()
                .ok_or(forge_domain::Error::NoDefaultProvider)?,
        };

        Ok(config
            .model
            .get(provider_id)
            .cloned()
            .ok_or_else(|| forge_domain::Error::no_default_model(*provider_id))?)
    }

    async fn set_default_model(&self, model: ModelId) -> anyhow::Result<()> {
        let provider_id = self
            .infra
            .get_app_config()
            .await?
            .provider
            .ok_or(forge_domain::Error::NoDefaultProvider)?;

        self.update(|config| {
            config.model.insert(provider_id, model.clone());
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use forge_domain::{
        AnyProvider, AppConfig, MigrationResult, Model, Models, Provider, ProviderId,
        ProviderResponse,
    };
    use pretty_assertions::assert_eq;
    use url::Url;

    use super::*;

    #[derive(Clone)]
    struct MockInfra {
        app_config: Arc<Mutex<AppConfig>>,
        providers: Vec<Provider<Url>>,
    }

    impl MockInfra {
        fn new() -> Self {
            Self {
                app_config: Arc::new(Mutex::new(AppConfig::default())),
                providers: vec![
                    Provider {
                        id: ProviderId::OpenAI,
                        response: ProviderResponse::OpenAI,
                        url: Url::parse("https://api.openai.com").unwrap(),
                        credential: Some(forge_domain::AuthCredential {
                            id: ProviderId::OpenAI,
                            auth_details: forge_domain::AuthDetails::ApiKey(
                                forge_domain::ApiKey::from("test-key".to_string()),
                            ),
                            url_params: HashMap::new(),
                        }),
                        auth_methods: vec![forge_domain::AuthMethod::ApiKey],
                        url_params: vec![],
                        models: Models::Hardcoded(vec![Model {
                            id: "gpt-4".to_string().into(),
                            name: Some("GPT-4".to_string()),
                            description: None,
                            context_length: Some(8192),
                            tools_supported: Some(true),
                            supports_parallel_tool_calls: Some(true),
                            supports_reasoning: Some(false),
                        }]),
                    },
                    Provider {
                        id: ProviderId::Anthropic,
                        response: ProviderResponse::Anthropic,
                        url: Url::parse("https://api.anthropic.com").unwrap(),
                        auth_methods: vec![forge_domain::AuthMethod::ApiKey],
                        url_params: vec![],
                        credential: Some(forge_domain::AuthCredential {
                            id: ProviderId::Anthropic,
                            auth_details: forge_domain::AuthDetails::ApiKey(
                                forge_domain::ApiKey::from("test-key".to_string()),
                            ),
                            url_params: HashMap::new(),
                        }),
                        models: Models::Hardcoded(vec![Model {
                            id: "claude-3".to_string().into(),
                            name: Some("Claude 3".to_string()),
                            description: None,
                            context_length: Some(200000),
                            tools_supported: Some(true),
                            supports_parallel_tool_calls: Some(true),
                            supports_reasoning: Some(true),
                        }]),
                    },
                ],
            }
        }
    }

    #[async_trait::async_trait]
    impl AppConfigRepository for MockInfra {
        async fn get_app_config(&self) -> anyhow::Result<AppConfig> {
            Ok(self.app_config.lock().unwrap().clone())
        }

        async fn set_app_config(&self, config: &AppConfig) -> anyhow::Result<()> {
            *self.app_config.lock().unwrap() = config.clone();
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockInfra {
        async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>> {
            Ok(self
                .providers
                .iter()
                .map(|p| AnyProvider::Url(p.clone()))
                .collect())
        }

        async fn get_provider(&self, id: ProviderId) -> anyhow::Result<Provider<Url>> {
            self.providers
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Provider not found"))
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

        async fn migrate_env_credentials(&self) -> anyhow::Result<Option<MigrationResult>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn test_get_default_provider_when_none_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture));

        let actual = service.get_default_provider().await?;
        let expected = ProviderId::OpenAI;

        assert_eq!(actual.id, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_default_provider_when_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        service.set_default_provider(ProviderId::Anthropic).await?;
        let actual = service.get_default_provider().await?;
        let expected = ProviderId::Anthropic;

        assert_eq!(actual.id, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_default_provider_when_configured_provider_not_available() -> anyhow::Result<()>
    {
        let mut fixture = MockInfra::new();
        // Remove OpenAI from available providers but keep it in config
        fixture.providers.retain(|p| p.id != ProviderId::OpenAI);
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        // Set OpenAI as the default provider in config
        service.set_default_provider(ProviderId::OpenAI).await?;

        // Should fall back to first available provider (Anthropic) since OpenAI is not
        // available
        let actual = service.get_default_provider().await?;
        let expected = ProviderId::Anthropic;

        assert_eq!(actual.id, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_set_default_provider() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        service.set_default_provider(ProviderId::Anthropic).await?;

        let config = fixture.get_app_config().await?;
        let actual = config.provider;
        let expected = Some(ProviderId::Anthropic);

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_default_model_when_none_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture));

        let result = service.get_provider_model(Some(&ProviderId::OpenAI)).await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_default_model_when_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        // Set OpenAI as the default provider first
        service.set_default_provider(ProviderId::OpenAI).await?;
        service
            .set_default_model("gpt-4".to_string().into())
            .await?;
        let actual = service
            .get_provider_model(Some(&ProviderId::OpenAI))
            .await?;
        let expected = "gpt-4".to_string().into();

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_set_default_model() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        // Set OpenAI as the default provider first
        service.set_default_provider(ProviderId::OpenAI).await?;
        service
            .set_default_model("gpt-4".to_string().into())
            .await?;

        let config = fixture.get_app_config().await?;
        let actual = config.model.get(&ProviderId::OpenAI).cloned();
        let expected = Some("gpt-4".to_string().into());

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_set_multiple_default_models() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        // Set models for different providers by switching active provider
        service.set_default_provider(ProviderId::OpenAI).await?;
        service
            .set_default_model("gpt-4".to_string().into())
            .await?;

        service.set_default_provider(ProviderId::Anthropic).await?;
        service
            .set_default_model("claude-3".to_string().into())
            .await?;

        let config = fixture.get_app_config().await?;
        let actual = config.model;
        let mut expected = HashMap::new();
        expected.insert(ProviderId::OpenAI, "gpt-4".to_string().into());
        expected.insert(ProviderId::Anthropic, "claude-3".to_string().into());

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_get_first_available_provider_is_deterministic() -> anyhow::Result<()> {
        // Setup mock with multiple providers
        // Without sorting in the repository layer, the order from
        // filter_map().collect() would be non-deterministic This test verifies
        // that get_first_available_provider always returns the same provider

        // Create multiple service instances to expose potential non-deterministic
        // behavior Each instance will have a different provider order without
        // sorting
        let mut all_first_providers = std::collections::HashSet::new();

        for _ in 0..100 {
            let fixture = MockInfra::new();
            let service = ForgeAppConfigService::new(Arc::new(fixture));
            let first_provider = service.get_first_available_provider().await?;
            all_first_providers.insert(first_provider.id);
        }

        // With sorting in the repository layer, we should always get the same provider
        assert_eq!(
            all_first_providers.len(),
            1,
            "get_first_available_provider should always return the same provider consistently across multiple service instances, got: {:?}",
            all_first_providers
        );

        // Verify it's always OpenAI (first in ProviderId enum after Forge)
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture));
        let first_provider = service.get_first_available_provider().await?;
        assert_eq!(
            first_provider.id,
            ProviderId::OpenAI,
            "First provider should be OpenAI (first in ProviderId enum order after Forge)"
        );

        Ok(())
    }
}
