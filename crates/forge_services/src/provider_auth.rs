use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_app::{AuthStrategy, ProviderAuthService, StrategyFactory};
use forge_domain::{
    AuthContextRequest, AuthContextResponse, AuthMethod, Provider, ProviderId, ProviderRepository,
    URLParam, URLParamValue, URLParameters,
};

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

impl<I> ForgeProviderAuthService<I>
where
    I: ProviderRepository + Send + Sync + 'static,
{
    async fn get_url_param_config(
        &self,
        provider_id: &ProviderId,
    ) -> anyhow::Result<(Vec<URLParam>, HashMap<URLParam, URLParamValue>)> {
        let providers = self.infra.get_all_providers().await?;
        let provider = providers
            .iter()
            .find(|p| p.id() == provider_id.clone())
            .ok_or_else(|| forge_domain::Error::provider_not_available(provider_id.clone()))?;

        let defaults = self
            .infra
            .get_provider_url_param_defaults(provider_id)
            .await?;
        Ok((provider.url_params().to_vec(), defaults))
    }

    fn merge_url_params(
        defaults: &HashMap<URLParam, URLParamValue>,
        existing: Option<&forge_domain::AuthCredential>,
    ) -> Option<URLParameters> {
        let mut params = defaults.clone();
        if let Some(existing) = existing {
            params.extend(existing.url_params.clone());
        }

        (!params.is_empty()).then_some(params.into())
    }

    fn attach_url_params_to_request(
        request: &mut AuthContextRequest,
        required_params: &[URLParam],
        defaults: &HashMap<URLParam, URLParamValue>,
        existing_credential: Option<&forge_domain::AuthCredential>,
    ) {
        let default_params = (!defaults.is_empty()).then_some(defaults.clone().into());
        let existing_params = Self::merge_url_params(defaults, existing_credential);

        match request {
            AuthContextRequest::ApiKey(api_key_request) => {
                api_key_request.required_params = required_params.to_vec();
                api_key_request.default_params = default_params.clone();
                api_key_request.existing_params = existing_params.clone();
            }
            AuthContextRequest::Code(code_request) => {
                code_request.required_params = required_params.to_vec();
                code_request.default_params = default_params.clone();
                code_request.existing_params = existing_params.clone();
            }
            AuthContextRequest::DeviceCode(device_request) => {
                device_request.required_params = required_params.to_vec();
                device_request.default_params = default_params;
                device_request.existing_params = existing_params;
            }
        }
    }

    fn response_url_params(
        auth_context_response: &AuthContextResponse,
    ) -> HashMap<URLParam, URLParamValue> {
        match auth_context_response {
            AuthContextResponse::ApiKey(response) => response.response.url_params.clone(),
            AuthContextResponse::Code(response) => response.response.url_params.clone(),
            AuthContextResponse::DeviceCode(response) => response.response.url_params.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<I> ProviderAuthService for ForgeProviderAuthService<I>
where
    I: StrategyFactory + ProviderRepository + Send + Sync + 'static,
{
    /// Initialize authentication flow for a provider
    async fn init_provider_auth(
        &self,
        provider_id: ProviderId,
        auth_method: AuthMethod,
    ) -> anyhow::Result<AuthContextRequest> {
        let (required_params, default_params) = self.get_url_param_config(&provider_id).await?;
        let existing_credential = self.infra.get_credential(&provider_id).await?;

        // Create appropriate strategy and initialize
        let strategy = self.infra.create_auth_strategy(
            provider_id.clone(),
            auth_method.clone(),
            required_params.clone(),
        )?;
        let mut request = strategy.init().await?;

        Self::attach_url_params_to_request(
            &mut request,
            &required_params,
            &default_params,
            existing_credential.as_ref(),
        );

        // Only prefill API keys for direct API-key based flows.
        if let AuthContextRequest::ApiKey(ref mut api_key_request) = request
            && let Some(existing_credential) = existing_credential.as_ref()
            && let Some(key) = existing_credential.auth_details.api_key()
        {
            let is_adc_marker = key.as_ref() == "google_adc_marker";
            let requesting_adc = matches!(auth_method, AuthMethod::GoogleAdc);

            if (requesting_adc && is_adc_marker) || (!requesting_adc && !is_adc_marker) {
                api_key_request.api_key = Some(key.clone());
            }
        }

        Ok(request)
    }

    /// Complete authentication flow for a provider
    async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        auth_context_response: AuthContextResponse,
        _timeout: Duration,
    ) -> anyhow::Result<()> {
        // Extract auth method from context response
        // For ApiKey responses, we need to check if it's Google ADC or regular API key
        let auth_method = match &auth_context_response {
            AuthContextResponse::ApiKey(response) => {
                // Check if provider supports Google ADC and if it's the Google ADC marker
                let is_vertex_provider = provider_id == forge_domain::ProviderId::VERTEX_AI
                    || provider_id == forge_domain::ProviderId::VERTEX_AI_ANTHROPIC;
                if is_vertex_provider && response.response.api_key.as_ref() == "google_adc_marker" {
                    // Vertex AI uses Google ADC
                    forge_domain::AuthMethod::google_adc()
                } else {
                    // Regular API key
                    forge_domain::AuthMethod::ApiKey
                }
            }
            AuthContextResponse::Code(ctx) => {
                AuthMethod::OAuthCode(ctx.request.oauth_config.clone())
            }
            AuthContextResponse::DeviceCode(ctx) => {
                if provider_id == forge_domain::ProviderId::CODEX {
                    AuthMethod::CodexDevice(ctx.request.oauth_config.clone())
                } else {
                    AuthMethod::OAuthDevice(ctx.request.oauth_config.clone())
                }
            }
        };

        let (required_params, _) = self.get_url_param_config(&provider_id).await?;
        let response_url_params = Self::response_url_params(&auth_context_response);

        // Create strategy and complete authentication
        let strategy =
            self.infra
                .create_auth_strategy(provider_id.clone(), auth_method, required_params)?;
        let mut credential = strategy.complete(auth_context_response).await?;
        credential.url_params.extend(response_url_params);

        // Store credential
        self.infra.upsert_credential(credential).await
    }

    /// Refreshes provider credentials if they're about to expire.
    /// Checks if credential needs refresh (5 minute buffer before expiry),
    /// iterates through provider's auth methods, and attempts to refresh.
    /// Returns the provider with updated credentials, or original if refresh
    /// fails or isn't needed.
    async fn refresh_provider_credential(
        &self,
        mut provider: Provider<url::Url>,
    ) -> anyhow::Result<Provider<url::Url>> {
        // Check if credential needs refresh (5 minute buffer before expiry)
        if let Some(credential) = &provider.credential {
            let buffer = chrono::Duration::minutes(5);

            if credential.needs_refresh(buffer) {
                // Iterate through auth methods and try to refresh
                for auth_method in &provider.auth_methods {
                    match auth_method {
                        AuthMethod::OAuthDevice(_)
                        | AuthMethod::OAuthCode(_)
                        | AuthMethod::CodexDevice(_)
                        | AuthMethod::GoogleAdc => {
                            // Get existing credential
                            let existing_credential =
                                self.infra.get_credential(&provider.id).await?.ok_or_else(
                                    || forge_domain::Error::ProviderNotAvailable {
                                        provider: provider.id.clone(),
                                    },
                                )?;

                            // Get required params (only used for API key, but needed for factory)
                            let required_params = if matches!(auth_method, AuthMethod::ApiKey) {
                                provider.url_params.clone()
                            } else {
                                vec![]
                            };

                            // Create strategy and refresh credential
                            if let Ok(strategy) = self.infra.create_auth_strategy(
                                provider.id.clone(),
                                auth_method.clone(),
                                required_params,
                            ) {
                                match strategy.refresh(&existing_credential).await {
                                    Ok(refreshed) => {
                                        // Store refreshed credential
                                        if self
                                            .infra
                                            .upsert_credential(refreshed.clone())
                                            .await
                                            .is_err()
                                        {
                                            continue;
                                        }

                                        // Update provider with refreshed credential
                                        provider.credential = Some(refreshed);
                                        break; // Success, stop trying other methods
                                    }
                                    Err(_) => {
                                        // If refresh fails, continue with
                                        // existing credentials
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(provider)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use forge_app::{AuthStrategy, StrategyFactory};
    use forge_domain::{
        AnyProvider, AuthCredential, AuthDetails, AuthMethod, OAuthConfig, OAuthTokens, Provider,
        ProviderId, ProviderRepository, ProviderResponse, ProviderTemplate, ProviderType, Template,
        URLParam, URLParamValue, URLParameters,
    };
    use pretty_assertions::assert_eq;
    use url::Url;

    use super::*;

    #[derive(Clone)]
    struct MockStrategy {
        init_request: AuthContextRequest,
        complete_credential: AuthCredential,
    }

    #[async_trait::async_trait]
    impl AuthStrategy for MockStrategy {
        async fn init(&self) -> anyhow::Result<AuthContextRequest> {
            Ok(self.init_request.clone())
        }

        async fn complete(
            &self,
            _context_response: AuthContextResponse,
        ) -> anyhow::Result<AuthCredential> {
            Ok(self.complete_credential.clone())
        }

        async fn refresh(&self, credential: &AuthCredential) -> anyhow::Result<AuthCredential> {
            Ok(credential.clone())
        }
    }

    struct MockInfra {
        providers: Vec<AnyProvider>,
        default_url_params: HashMap<ProviderId, HashMap<URLParam, URLParamValue>>,
        existing_credential: Option<AuthCredential>,
        stored_credential: Mutex<Option<AuthCredential>>,
        init_request: AuthContextRequest,
        complete_credential: AuthCredential,
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockInfra {
        async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>> {
            Ok(self.providers.clone())
        }

        async fn get_provider(&self, _id: ProviderId) -> anyhow::Result<ProviderTemplate> {
            Err(anyhow::anyhow!("unused in tests"))
        }

        async fn get_provider_url_param_defaults(
            &self,
            id: &ProviderId,
        ) -> anyhow::Result<HashMap<URLParam, URLParamValue>> {
            Ok(self.default_url_params.get(id).cloned().unwrap_or_default())
        }

        async fn upsert_credential(&self, credential: AuthCredential) -> anyhow::Result<()> {
            *self.stored_credential.lock().unwrap() = Some(credential);
            Ok(())
        }

        async fn get_credential(&self, _id: &ProviderId) -> anyhow::Result<Option<AuthCredential>> {
            Ok(self.existing_credential.clone())
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

    impl StrategyFactory for MockInfra {
        type Strategy = MockStrategy;

        fn create_auth_strategy(
            &self,
            _provider_id: ProviderId,
            _auth_method: forge_domain::AuthMethod,
            _required_params: Vec<forge_domain::URLParam>,
        ) -> anyhow::Result<Self::Strategy> {
            Ok(MockStrategy {
                init_request: self.init_request.clone(),
                complete_credential: self.complete_credential.clone(),
            })
        }
    }

    fn oauth_config_fixture() -> OAuthConfig {
        OAuthConfig {
            client_id: "client-id".to_string().into(),
            auth_url: Url::parse("https://example.com/auth").unwrap(),
            token_url: Url::parse("https://example.com/token").unwrap(),
            scopes: vec!["scope".to_string()],
            redirect_uri: None,
            use_pkce: false,
            token_refresh_url: None,
            extra_auth_params: None,
            custom_headers: None,
        }
    }

    fn provider_fixture(
        provider_id: ProviderId,
        auth_method: AuthMethod,
        url_param: &str,
    ) -> AnyProvider {
        AnyProvider::Template(Provider {
            id: provider_id,
            provider_type: ProviderType::Llm,
            response: Some(ProviderResponse::Anthropic),
            url: Template::new(format!("https://{{{{{url_param}}}}}/messages")),
            models: None,
            auth_methods: vec![auth_method],
            url_params: vec![URLParam::from(url_param.to_string())],
            credential: None,
        })
    }

    #[tokio::test]
    async fn test_init_provider_auth_attaches_default_and_existing_url_params_to_oauth_request() {
        let fixture_param = URLParam::from("CLAUDE_CODE_BASE_URL".to_string());
        let fixture_default = URLParamValue::from("https://api.anthropic.com/v1".to_string());
        let fixture_existing = URLParamValue::from("https://gateway.example.com/v1".to_string());
        let fixture_config = oauth_config_fixture();
        let fixture_request = AuthContextRequest::Code(forge_domain::CodeRequest {
            authorization_url: Url::parse("https://example.com/authorize").unwrap(),
            state: "state".to_string().into(),
            pkce_verifier: None,
            oauth_config: fixture_config.clone(),
            required_params: vec![],
            default_params: None,
            existing_params: None,
        });
        let fixture_complete_credential = AuthCredential::new_oauth(
            ProviderId::CLAUDE_CODE,
            OAuthTokens::new(
                "oauth-token",
                None::<String>,
                chrono::Utc::now() + chrono::Duration::hours(1),
            ),
            fixture_config.clone(),
        );
        let fixture_existing_credential = AuthCredential::new_api_key(
            ProviderId::CLAUDE_CODE,
            forge_domain::ApiKey::from("gateway-key".to_string()),
        )
        .url_params(HashMap::from([(
            fixture_param.clone(),
            fixture_existing.clone(),
        )]));
        let fixture_provider = provider_fixture(
            ProviderId::CLAUDE_CODE,
            AuthMethod::OAuthCode(fixture_config.clone()),
            "CLAUDE_CODE_BASE_URL",
        );
        let fixture_defaults = HashMap::from([(
            ProviderId::CLAUDE_CODE,
            HashMap::from([(fixture_param.clone(), fixture_default.clone())]),
        )]);
        let fixture_infra = Arc::new(MockInfra {
            providers: vec![fixture_provider],
            default_url_params: fixture_defaults,
            existing_credential: Some(fixture_existing_credential),
            stored_credential: Mutex::new(None),
            init_request: fixture_request,
            complete_credential: fixture_complete_credential,
        });
        let fixture_service = ForgeProviderAuthService::new(fixture_infra);

        let actual = fixture_service
            .init_provider_auth(
                ProviderId::CLAUDE_CODE,
                AuthMethod::OAuthCode(fixture_config),
            )
            .await
            .unwrap();

        let AuthContextRequest::Code(actual) = actual else {
            panic!("Expected code auth request");
        };
        let expected_required = vec![fixture_param.clone()];
        let expected_default = Some(URLParameters::from(HashMap::from([(
            fixture_param.clone(),
            fixture_default,
        )])));
        let expected_existing = Some(URLParameters::from(HashMap::from([(
            fixture_param,
            fixture_existing,
        )])));

        assert_eq!(actual.required_params, expected_required);
        assert_eq!(actual.default_params, expected_default);
        assert_eq!(actual.existing_params, expected_existing);
    }

    #[tokio::test]
    async fn test_complete_provider_auth_merges_response_url_params_into_saved_credential() {
        let fixture_param = URLParam::from("CLAUDE_CODE_BASE_URL".to_string());
        let fixture_account_param = URLParam::from("chatgpt_account_id".to_string());
        let fixture_config = oauth_config_fixture();
        let fixture_provider = provider_fixture(
            ProviderId::CLAUDE_CODE,
            AuthMethod::OAuthCode(fixture_config.clone()),
            "CLAUDE_CODE_BASE_URL",
        );
        let fixture_complete_credential = AuthCredential {
            id: ProviderId::CLAUDE_CODE,
            auth_details: AuthDetails::OAuth {
                tokens: OAuthTokens::new(
                    "oauth-token",
                    None::<String>,
                    chrono::Utc::now() + chrono::Duration::hours(1),
                ),
                config: fixture_config.clone(),
            },
            url_params: HashMap::from([(
                fixture_account_param.clone(),
                "acct_123".to_string().into(),
            )]),
        };
        let fixture_request = AuthContextRequest::Code(forge_domain::CodeRequest {
            authorization_url: Url::parse("https://example.com/authorize").unwrap(),
            state: "state".to_string().into(),
            pkce_verifier: None,
            oauth_config: fixture_config.clone(),
            required_params: vec![],
            default_params: None,
            existing_params: None,
        });
        let fixture_infra = Arc::new(MockInfra {
            providers: vec![fixture_provider],
            default_url_params: HashMap::from([(
                ProviderId::CLAUDE_CODE,
                HashMap::from([(
                    fixture_param.clone(),
                    "https://api.anthropic.com/v1".to_string().into(),
                )]),
            )]),
            existing_credential: None,
            stored_credential: Mutex::new(None),
            init_request: fixture_request,
            complete_credential: fixture_complete_credential,
        });
        let fixture_service = ForgeProviderAuthService::new(fixture_infra.clone());
        let fixture_response = AuthContextResponse::code(
            forge_domain::CodeRequest {
                authorization_url: Url::parse("https://example.com/authorize").unwrap(),
                state: "state".to_string().into(),
                pkce_verifier: None,
                oauth_config: fixture_config.clone(),
                required_params: vec![fixture_param.clone()],
                default_params: None,
                existing_params: None,
            },
            "code-123",
            HashMap::from([(
                "CLAUDE_CODE_BASE_URL".to_string(),
                "https://gateway.example.com/v1".to_string(),
            )]),
        );

        fixture_service
            .complete_provider_auth(ProviderId::CLAUDE_CODE, fixture_response, Duration::ZERO)
            .await
            .unwrap();

        let actual = fixture_infra.stored_credential.lock().unwrap().clone();
        let expected = Some(
            fixture_infra
                .complete_credential
                .clone()
                .url_params(HashMap::from([
                    (fixture_account_param, "acct_123".to_string().into()),
                    (
                        fixture_param,
                        "https://gateway.example.com/v1".to_string().into(),
                    ),
                ])),
        );

        assert_eq!(actual, expected);
    }
}
