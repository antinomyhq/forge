use std::sync::Arc;
use std::time::Duration;

use forge_app::ProviderAuthService;
use forge_domain::{
    AuthContextRequest, AuthContextResponse, AuthCredential, AuthMethod, Provider, ProviderId,
    ProviderRepository,
};

use super::provider_auth_strategy::{AuthStrategy, create_auth_strategy};

/// Forge Provider Authentication Service
#[derive(Clone)]
pub struct ForgeProviderAuthService<I> {
    infra: Arc<I>,
}

impl<I> ForgeProviderAuthService<I> {
    /// Create a new provider authentication service
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<I> ProviderAuthService for ForgeProviderAuthService<I>
where
    I: ProviderRepository + Send + Sync + 'static,
{
    /// Initialize authentication flow for a provider
    async fn init_provider_auth(
        &self,
        provider_id: ProviderId,
        auth_method: AuthMethod,
    ) -> anyhow::Result<AuthContextRequest> {
        // Get required URL parameters for API key flow
        let required_params = if matches!(auth_method, AuthMethod::ApiKey) {
            self.infra
                .get_provider(provider_id)
                .await?
                .url_params
                .clone()
        } else {
            vec![]
        };

        // Create appropriate strategy and initialize
        let strategy = create_auth_strategy(provider_id, auth_method, required_params)?;
        strategy.init().await
    }

    /// Complete authentication flow for a provider
    async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        auth_context_response: AuthContextResponse,
        _timeout: Duration,
    ) -> anyhow::Result<()> {
        // Extract auth method from context response
        let auth_method = match &auth_context_response {
            AuthContextResponse::ApiKey(_) => AuthMethod::ApiKey,
            AuthContextResponse::Code(ctx) => {
                AuthMethod::OAuthCode(ctx.request.oauth_config.clone())
            }
            AuthContextResponse::DeviceCode(ctx) => {
                AuthMethod::OAuthDevice(ctx.request.oauth_config.clone())
            }
        };

        // Get required params for API key flow
        let required_params = if matches!(auth_method, AuthMethod::ApiKey) {
            self.infra
                .get_provider(provider_id)
                .await?
                .url_params
                .clone()
        } else {
            vec![]
        };

        // Create strategy and complete authentication
        let strategy = create_auth_strategy(provider_id, auth_method, required_params)?;
        let credential = strategy.complete(auth_context_response).await?;

        // Store credential
        self.infra.upsert_credential(credential).await
    }

    /// Refresh provider credential
    async fn refresh_provider_credential(
        &self,
        provider: &Provider<url::Url>,
        auth_method: AuthMethod,
    ) -> anyhow::Result<AuthCredential> {
        // Get existing credential
        let credential = self
            .infra
            .get_credential(&provider.id)
            .await?
            .ok_or_else(|| forge_domain::Error::ProviderNotAvailable { provider: provider.id })?;

        // Get required params (only used for API key, but needed for factory)
        let required_params = if matches!(auth_method, AuthMethod::ApiKey) {
            provider.url_params.clone()
        } else {
            vec![]
        };

        // Create strategy and refresh credential
        let strategy = create_auth_strategy(provider.id, auth_method, required_params)?;
        let refreshed = strategy.refresh(&credential).await?;

        // Store refreshed credential
        self.infra.upsert_credential(refreshed.clone()).await?;

        Ok(refreshed)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_domain::{OAuthConfig, ProviderEntry};
    use url::Url;

    use super::*;

    // Mock repository for testing
    struct MockRepository {
        providers: HashMap<ProviderId, Provider<Url>>,
        credentials: HashMap<ProviderId, AuthCredential>,
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockRepository {
        async fn get_all_providers(&self) -> anyhow::Result<Vec<ProviderEntry>> {
            Ok(self
                .providers
                .values()
                .cloned()
                .map(|p| ProviderEntry::Available(p))
                .collect())
        }

        async fn get_provider(&self, id: ProviderId) -> anyhow::Result<Provider<Url>> {
            self.providers
                .get(&id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Provider not found"))
        }

        async fn get_credential(&self, id: &ProviderId) -> anyhow::Result<Option<AuthCredential>> {
            Ok(self.credentials.get(id).cloned())
        }

        async fn upsert_credential(&self, _credential: AuthCredential) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn create_test_service() -> ForgeProviderAuthService<MockRepository> {
        let repo = MockRepository { providers: HashMap::new(), credentials: HashMap::new() };
        ForgeProviderAuthService::new(Arc::new(repo))
    }

    #[tokio::test]
    async fn test_init_api_key_auth() {
        let service = create_test_service();

        let result = service
            .init_provider_auth(ProviderId::OpenAI, AuthMethod::ApiKey)
            .await;

        // Should succeed even with empty provider (will get error from repo, but that's
        // expected in test)
        assert!(result.is_err()); // Provider not found is expected
    }

    #[tokio::test]
    async fn test_init_oauth_code_auth() {
        let service = create_test_service();
        let config = OAuthConfig {
            client_id: "test_client".to_string().into(),
            auth_url: Url::parse("https://example.com/auth").unwrap(),
            token_url: Url::parse("https://example.com/token").unwrap(),
            scopes: vec!["read".to_string()],
            redirect_uri: Some("https://example.com/callback".to_string()),
            use_pkce: true,
            token_refresh_url: None,
            extra_auth_params: None,
            custom_headers: None,
        };

        let result = service
            .init_provider_auth(ProviderId::OpenAI, AuthMethod::OAuthCode(config))
            .await
            .unwrap();

        assert!(matches!(result, AuthContextRequest::Code(_)));
    }
}
