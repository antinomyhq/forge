use std::sync::Arc;

use forge_app::AppConfigService;
use forge_domain::{AppConfig, AppConfigRepository, ModelId, ProviderId, ProviderRepository};

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
}

#[async_trait::async_trait]
impl<F: ProviderRepository + AppConfigRepository + Send + Sync> AppConfigService
    for ForgeAppConfigService<F>
{
    async fn get_default_provider(&self) -> anyhow::Result<ProviderId> {
        let app_config = self.infra.get_app_config().await?;
        app_config
            .provider
            .ok_or_else(|| forge_domain::Error::NoDefaultProvider.into())
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
            .ok_or_else(|| forge_domain::Error::no_default_model(provider_id.clone()))?)
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
        AnyProvider, AppConfig, ChatRepository, MigrationResult, Model, ModelSource, Provider,
        ProviderId, ProviderResponse, ProviderTemplate,
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
                        id: ProviderId::OPENAI,
                        provider_type: Default::default(),
                        response: Some(ProviderResponse::OpenAI),
                        url: Url::parse("https://api.openai.com").unwrap(),
                        credential: Some(forge_domain::AuthCredential {
                            id: ProviderId::OPENAI,
                            auth_details: forge_domain::AuthDetails::ApiKey(
                                forge_domain::ApiKey::from("test-key".to_string()),
                            ),
                            url_params: HashMap::new(),
                        }),
                        auth_methods: vec![forge_domain::AuthMethod::ApiKey],
                        url_params: vec![],
                        models: Some(ModelSource::Hardcoded(vec![Model {
                            id: "gpt-4".to_string().into(),
                            name: Some("GPT-4".to_string()),
                            description: None,
                            context_length: Some(8192),
                            tools_supported: Some(true),
                            supports_parallel_tool_calls: Some(true),
                            supports_reasoning: Some(false),
                        }])),
                    },
                    Provider {
                        id: ProviderId::ANTHROPIC,
                        provider_type: Default::default(),
                        response: Some(ProviderResponse::Anthropic),
                        url: Url::parse("https://api.anthropic.com").unwrap(),
                        auth_methods: vec![forge_domain::AuthMethod::ApiKey],
                        url_params: vec![],
                        credential: Some(forge_domain::AuthCredential {
                            id: ProviderId::ANTHROPIC,
                            auth_details: forge_domain::AuthDetails::ApiKey(
                                forge_domain::ApiKey::from("test-key".to_string()),
                            ),
                            url_params: HashMap::new(),
                        }),
                        models: Some(ModelSource::Hardcoded(vec![Model {
                            id: "claude-3".to_string().into(),
                            name: Some("Claude 3".to_string()),
                            description: None,
                            context_length: Some(200000),
                            tools_supported: Some(true),
                            supports_parallel_tool_calls: Some(true),
                            supports_reasoning: Some(true),
                        }])),
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
    impl ChatRepository for MockInfra {
        async fn chat(
            &self,
            _model_id: &forge_app::domain::ModelId,
            _context: forge_app::domain::Context,
            _provider: Provider<Url>,
        ) -> forge_app::domain::ResultStream<forge_app::domain::ChatCompletionMessage, anyhow::Error>
        {
            Ok(Box::pin(tokio_stream::iter(vec![])))
        }

        async fn models(
            &self,
            _provider: Provider<Url>,
        ) -> anyhow::Result<Vec<forge_app::domain::Model>> {
            Ok(vec![])
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

        async fn get_provider(&self, id: ProviderId) -> anyhow::Result<ProviderTemplate> {
            // Convert Provider<Url> to Provider<Template<...>> for testing
            self.providers
                .iter()
                .find(|p| p.id == id)
                .map(|p| Provider {
                    id: p.id.clone(),
                    provider_type: p.provider_type,
                    response: p.response.clone(),
                    url: forge_domain::Template::<forge_domain::URLParameters>::new(p.url.as_str()),
                    models: p.models.as_ref().map(|m| match m {
                        ModelSource::Url(url) => ModelSource::Url(forge_domain::Template::<
                            forge_domain::URLParameters,
                        >::new(
                            url.as_str()
                        )),
                        ModelSource::Hardcoded(list) => ModelSource::Hardcoded(list.clone()),
                    }),
                    auth_methods: p.auth_methods.clone(),
                    url_params: p.url_params.clone(),
                    credential: p.credential.clone(),
                })
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

        let result = service.get_default_provider().await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_default_provider_when_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        service.set_default_provider(ProviderId::ANTHROPIC).await?;
        let actual = service.get_default_provider().await?;
        let expected = ProviderId::ANTHROPIC;

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_default_provider_when_configured_provider_not_available() -> anyhow::Result<()>
    {
        let mut fixture = MockInfra::new();
        // Remove OpenAI from available providers but keep it in config
        fixture.providers.retain(|p| p.id != ProviderId::OPENAI);
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        // Set OpenAI as the default provider in config
        service.set_default_provider(ProviderId::OPENAI).await?;

        // Should return the provider ID even if provider is not available
        // Validation happens when getting the actual provider via ProviderService
        let result = service.get_default_provider().await?;

        assert_eq!(result, ProviderId::OPENAI);
        Ok(())
    }

    #[tokio::test]
    async fn test_set_default_provider() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        service.set_default_provider(ProviderId::ANTHROPIC).await?;

        let config = fixture.get_app_config().await?;
        let actual = config.provider;
        let expected = Some(ProviderId::ANTHROPIC);

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_default_model_when_none_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture));

        let result = service.get_provider_model(Some(&ProviderId::OPENAI)).await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_default_model_when_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        // Set OpenAI as the default provider first
        service.set_default_provider(ProviderId::OPENAI).await?;
        service
            .set_default_model("gpt-4".to_string().into())
            .await?;
        let actual = service
            .get_provider_model(Some(&ProviderId::OPENAI))
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
        service.set_default_provider(ProviderId::OPENAI).await?;
        service
            .set_default_model("gpt-4".to_string().into())
            .await?;

        let config = fixture.get_app_config().await?;
        let actual = config.model.get(&ProviderId::OPENAI).cloned();
        let expected = Some("gpt-4".to_string().into());

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_set_multiple_default_models() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        // Set models for different providers by switching active provider
        service.set_default_provider(ProviderId::OPENAI).await?;
        service
            .set_default_model("gpt-4".to_string().into())
            .await?;

        service.set_default_provider(ProviderId::ANTHROPIC).await?;
        service
            .set_default_model("claude-3".to_string().into())
            .await?;

        let config = fixture.get_app_config().await?;
        let actual = config.model;
        let mut expected = HashMap::new();
        expected.insert(ProviderId::OPENAI, "gpt-4".to_string().into());
        expected.insert(ProviderId::ANTHROPIC, "claude-3".to_string().into());

        assert_eq!(actual, expected);
        Ok(())
    }
}
