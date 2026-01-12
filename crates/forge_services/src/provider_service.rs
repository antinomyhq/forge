use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use forge_app::ProviderService;
use forge_app::domain::{
    AnyProvider, ChatCompletionMessage, Model, ModelId, ProviderId, ResultStream,
};
use forge_domain::{
    AuthCredential, ChatRepository, Context, MigrationResult, ModelRepository, ModelSource,
    Provider, ProviderRepository, ProviderTemplate,
};
use tokio::sync::Mutex;
use url::Url;

/// Service layer wrapper for ProviderRepository that handles model caching and
/// template rendering
pub struct ForgeProviderService<R> {
    repository: Arc<R>,
    cached_models: Arc<Mutex<HashMap<ProviderId, Vec<Model>>>>,
}

impl<R> ForgeProviderService<R> {
    /// Creates a new ForgeProviderService instance
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            cached_models: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Renders a URL template with provided parameters
    fn render_url_template(
        &self,
        template: &str,
        params: &HashMap<forge_domain::URLParam, forge_domain::URLParamValue>,
    ) -> Result<Url> {
        let template_data: HashMap<&str, &str> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let handlebars = forge_app::TemplateEngine::handlebar_instance();
        let rendered = handlebars.render_template(template, &template_data)?;

        Ok(Url::parse(&rendered)?)
    }

    /// Renders a provider from template to fully resolved URLs
    fn render_provider(&self, template_provider: ProviderTemplate) -> Result<Provider<Url>> {
        let credential = template_provider
            .credential
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Provider has no credential"))?;

        // Render main URL
        let url =
            self.render_url_template(&template_provider.url.template, &credential.url_params)?;

        // Render model source URLs
        let models = template_provider.models.as_ref().and_then(|m| match m {
            ModelSource::Url(template) => {
                let model_url = self
                    .render_url_template(&template.template, &credential.url_params)
                    .ok();
                model_url.map(ModelSource::Url)
            }
            ModelSource::Hardcoded(list) => Some(ModelSource::Hardcoded(list.clone())),
        });

        Ok(Provider {
            id: template_provider.id,
            provider_type: template_provider.provider_type,
            response: template_provider.response,
            url,
            models,
            auth_methods: template_provider.auth_methods,
            url_params: template_provider.url_params,
            credential: template_provider.credential,
        })
    }
}

#[async_trait::async_trait]
impl<R: ChatRepository + ProviderRepository + ModelRepository> ProviderService
    for ForgeProviderService<R>
{
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        // Repository builds client on each call (no caching at repository level)
        self.repository.chat(model_id, context, provider).await
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

        // Models not in cache, fetch from repository
        let models = self.repository.list_models(&provider.id).await?;

        // Cache the models for this provider
        {
            let mut models_guard = self.cached_models.lock().await;
            models_guard.insert(provider_id, models.clone());
        }

        Ok(models)
    }

    async fn get_all_providers(&self) -> Result<Vec<AnyProvider>> {
        let providers = self.repository.get_all_providers().await?;

        // Render configured providers from Template to Url
        let rendered_providers = providers
            .into_iter()
            .map(|provider| {
                // If provider is a Template with credentials, render it to Url
                if let AnyProvider::Template(template_provider) = &provider
                    && template_provider.is_configured()
                {
                    // Clone and render the provider
                    if let Ok(rendered) = self.render_provider(template_provider.clone()) {
                        return AnyProvider::Url(rendered);
                    }
                }
                // Otherwise return as-is
                provider
            })
            .collect();

        Ok(rendered_providers)
    }

    async fn get_provider(&self, id: ProviderId) -> Result<Provider<Url>> {
        let template_provider = self.repository.get_provider(id).await?;
        self.render_provider(template_provider)
    }

    async fn upsert_credential(&self, credential: AuthCredential) -> Result<()> {
        self.repository.upsert_credential(credential).await
    }

    async fn remove_credential(&self, id: &ProviderId) -> Result<()> {
        self.repository.remove_credential(id).await
    }

    async fn migrate_env_credentials(&self) -> Result<Option<MigrationResult>> {
        self.repository.migrate_env_credentials().await
    }
}

#[cfg(test)]
mod tests {
    use forge_app::ProviderService;
    use forge_app::domain::ProviderId;
    use forge_domain::{
        AuthDetails, AuthMethod, InputModality, ModelSource, ProviderType, Template,
    };
    use pretty_assertions::assert_eq;

    use super::*;

    // Mock repository for testing
    struct MockProviderRepository {
        models: Vec<Model>,
        call_count: Arc<Mutex<usize>>,
        providers: Vec<AnyProvider>,
    }

    impl MockProviderRepository {
        fn new(models: Vec<Model>) -> Self {
            Self {
                models,
                call_count: Arc::new(Mutex::new(0)),
                providers: vec![],
            }
        }

        fn with_providers(mut self, providers: Vec<AnyProvider>) -> Self {
            self.providers = providers;
            self
        }

        async fn get_call_count(&self) -> usize {
            *self.call_count.lock().await
        }
    }

    #[async_trait::async_trait]
    impl ChatRepository for MockProviderRepository {
        async fn chat(
            &self,
            _model_id: &ModelId,
            _context: Context,
            _provider: Provider<Url>,
        ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
            Ok(Box::pin(tokio_stream::empty()))
        }

        async fn models(&self, _provider: Provider<Url>) -> Result<Vec<Model>> {
            let mut count = self.call_count.lock().await;
            *count += 1;
            Ok(self.models.clone())
        }
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockProviderRepository {
        async fn get_all_providers(&self) -> Result<Vec<AnyProvider>> {
            Ok(self.providers.clone())
        }

        async fn get_provider(&self, _id: ProviderId) -> Result<ProviderTemplate> {
            Ok(test_template_provider())
        }

        async fn get_credential(&self, _id: &ProviderId) -> Result<Option<AuthCredential>> {
            Ok(None)
        }

        async fn upsert_credential(&self, _credential: AuthCredential) -> Result<()> {
            Ok(())
        }

        async fn remove_credential(&self, _id: &ProviderId) -> Result<()> {
            Ok(())
        }

        async fn migrate_env_credentials(&self) -> Result<Option<MigrationResult>> {
            Ok(None)
        }
    }

    #[async_trait::async_trait]
    impl ModelRepository for MockProviderRepository {
        async fn get_model(
            &self,
            _provider_id: &ProviderId,
            _model_id: &ModelId,
        ) -> Result<Option<Model>> {
            Ok(self.models.first().cloned())
        }

        async fn list_models(&self, _provider_id: &ProviderId) -> Result<Vec<Model>> {
            let mut count = self.call_count.lock().await;
            *count += 1;
            Ok(self.models.clone())
        }
    }

    fn test_provider() -> Provider<Url> {
        Provider {
            id: ProviderId::OPENAI,
            provider_type: ProviderType::Llm,
            response: Some(forge_app::domain::ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            auth_methods: vec![AuthMethod::ApiKey],
            url_params: vec![],
            credential: Some(AuthCredential {
                id: ProviderId::OPENAI,
                auth_details: AuthDetails::ApiKey(forge_domain::ApiKey::from(
                    "test-key".to_string(),
                )),
                url_params: HashMap::new(),
            }),
            models: Some(ModelSource::Url(
                Url::parse("https://api.openai.com/v1/models").unwrap(),
            )),
        }
    }

    fn test_template_provider() -> ProviderTemplate {
        Provider {
            id: ProviderId::OPENAI,
            provider_type: ProviderType::Llm,
            response: Some(forge_app::domain::ProviderResponse::OpenAI),
            url: Template::<forge_domain::URLParameters>::new(
                "https://api.openai.com/v1/chat/completions",
            ),
            auth_methods: vec![AuthMethod::ApiKey],
            url_params: vec![],
            credential: Some(AuthCredential {
                id: ProviderId::OPENAI,
                auth_details: AuthDetails::ApiKey(forge_domain::ApiKey::from(
                    "test-key".to_string(),
                )),
                url_params: HashMap::new(),
            }),
            models: Some(ModelSource::Url(
                Template::<forge_domain::URLParameters>::new("https://api.openai.com/v1/models"),
            )),
        }
    }

    fn test_model(id: &str) -> Model {
        Model {
            id: ModelId::from(id),
            name: Some(id.to_string()),
            description: None,
            context_length: Some(4096),
            tools_supported: Some(true),
            supports_parallel_tool_calls: Some(true),
            supports_reasoning: Some(false),
            input_modalities: vec![InputModality::Text],
        }
    }

    #[tokio::test]
    async fn test_cache_initialization() {
        let repository = Arc::new(MockProviderRepository::new(vec![]));
        let service = ForgeProviderService::new(repository);

        // Verify cache is initialized as empty
        let cache = service.cached_models.lock().await;
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_models_caches_on_first_call() {
        let models = vec![test_model("gpt-4"), test_model("gpt-3.5-turbo")];
        let repository = Arc::new(MockProviderRepository::new(models.clone()));
        let service = ForgeProviderService::new(repository.clone());
        let provider = test_provider();

        // First call - should fetch from repository
        let actual = service.models(provider.clone()).await.unwrap();
        assert_eq!(actual, models);
        assert_eq!(repository.get_call_count().await, 1);

        // Verify cache is populated
        let cache = service.cached_models.lock().await;
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key(&ProviderId::OPENAI));
    }

    #[tokio::test]
    async fn test_models_returns_cached_on_second_call() {
        let models = vec![test_model("gpt-4"), test_model("gpt-3.5-turbo")];
        let repository = Arc::new(MockProviderRepository::new(models.clone()));
        let service = ForgeProviderService::new(repository.clone());
        let provider = test_provider();

        // First call - populates cache
        let _ = service.models(provider.clone()).await.unwrap();
        assert_eq!(repository.get_call_count().await, 1);

        // Second call - should use cache, not call repository
        let actual = service.models(provider.clone()).await.unwrap();
        assert_eq!(actual, models);
        assert_eq!(repository.get_call_count().await, 1); // Still 1, no additional call
    }

    #[tokio::test]
    async fn test_models_caches_per_provider() {
        let openai_models = vec![test_model("gpt-4")];
        let repository = Arc::new(MockProviderRepository::new(openai_models.clone()));
        let service = ForgeProviderService::new(repository.clone());

        let openai_provider = test_provider();
        let mut anthropic_provider = test_provider();
        anthropic_provider.id = ProviderId::ANTHROPIC;

        // Fetch models for OpenAI
        let _ = service.models(openai_provider).await.unwrap();

        // Fetch models for Anthropic
        let _ = service.models(anthropic_provider).await.unwrap();

        // Verify both providers are cached separately
        let cache = service.cached_models.lock().await;
        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key(&ProviderId::OPENAI));
        assert!(cache.contains_key(&ProviderId::ANTHROPIC));
    }

    #[tokio::test]
    async fn test_service_initialization_with_default() {
        let repository = Arc::new(MockProviderRepository::new(vec![]));
        let service = ForgeProviderService::new(repository);

        // Verify service is properly initialized
        let cache = service.cached_models.lock().await;
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_get_all_providers_renders_configured_providers() {
        let configured = test_template_provider();
        let unconfigured = Provider { credential: None, ..test_template_provider() };

        let repository = Arc::new(MockProviderRepository::new(vec![]).with_providers(vec![
            AnyProvider::Template(configured),
            AnyProvider::Template(unconfigured),
        ]));

        let service = ForgeProviderService::new(repository);
        let actual = service.get_all_providers().await.unwrap();

        assert_eq!(actual.len(), 2);
        assert!(matches!(actual[0], AnyProvider::Url(_)));
        assert!(matches!(actual[1], AnyProvider::Template(_)));

        if let AnyProvider::Url(provider) = &actual[0] {
            assert_eq!(
                provider.url.as_str(),
                "https://api.openai.com/v1/chat/completions"
            );
        }
    }
}
