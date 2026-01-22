use std::sync::Arc;

use forge_domain::{AuthFlowRepository, ProviderRepository, WorkspaceAuth};

/// Service that ensures user is authenticated before allowing operations
///
/// This service acts as a gate, checking for valid authentication and
/// initiating the auth flow if needed.
pub struct AuthGateService<R> {
    infra: Arc<R>,
}

impl<R> AuthGateService<R> {
    pub fn new(infra: Arc<R>) -> Self {
        Self { infra }
    }
}

impl<R: ProviderRepository + AuthFlowRepository> AuthGateService<R> {
    /// Ensure user is authenticated, returning stored auth or initiating flow
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Token validation fails
    /// - Authentication flow fails
    /// - Storage operations fail
    pub async fn ensure_authenticated(&self) -> anyhow::Result<WorkspaceAuth> {
        // Try to get stored auth
        if let Some(auth) = self.infra.get_auth().await? {
            // Validate token is still valid
            if self.validate_token(&auth.token).await? {
                return Ok(auth);
            }
            // Token invalid, clear it
            self.infra.clear_auth().await?;
        }

        // No valid auth, need to authenticate
        Err(anyhow::anyhow!(
            "Authentication required. Please run 'forge auth login' to authenticate."
        ))
    }

    /// Check if user is currently authenticated with a valid token
    ///
    /// # Errors
    ///
    /// Returns error if storage access fails
    pub async fn is_authenticated(&self) -> anyhow::Result<bool> {
        if let Some(auth) = self.infra.get_auth().await? {
            self.validate_token(&auth.token).await
        } else {
            Ok(false)
        }
    }

    /// Get current authentication if exists
    ///
    /// # Errors
    ///
    /// Returns error if storage access fails
    pub async fn get_current_auth(&self) -> anyhow::Result<Option<WorkspaceAuth>> {
        self.infra.get_auth().await
    }

    /// Logout - clear stored authentication
    ///
    /// # Errors
    ///
    /// Returns error if storage operations fail
    pub async fn logout(&self) -> anyhow::Result<()> {
        self.infra.clear_auth().await
    }

    /// Validate token by attempting to list API keys
    ///
    /// # Errors
    ///
    /// Returns error if validation request fails
    async fn validate_token(&self, token: &forge_domain::ApiKey) -> anyhow::Result<bool> {
        // Try to call get_api_keys as a health check
        match self.infra.get_api_keys(token).await {
            Ok(_) => Ok(true),
            Err(e) => {
                // Check if error is authentication-related
                let error_msg = e.to_string().to_lowercase();
                if error_msg.contains("unauthenticated")
                    || error_msg.contains("unauthorized")
                    || error_msg.contains("invalid")
                    || error_msg.contains("expired")
                {
                    Ok(false)
                } else {
                    // Other errors should be propagated
                    Err(e)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use forge_domain::{ApiKey, ApiKeyInfo, AuthFlowLoginInfo, InitFlowResponse, UserId};
    use pretty_assertions::assert_eq;

    use super::*;

    #[derive(Clone)]
    struct MockInfra {
        stored_auth: Arc<std::sync::Mutex<Option<WorkspaceAuth>>>,
        token_valid: bool,
    }

    impl MockInfra {
        fn new() -> Self {
            Self {
                stored_auth: Arc::new(std::sync::Mutex::new(None)),
                token_valid: true,
            }
        }

        fn with_auth(auth: WorkspaceAuth) -> Self {
            Self {
                stored_auth: Arc::new(std::sync::Mutex::new(Some(auth))),
                token_valid: true,
            }
        }

        fn with_invalid_token(auth: WorkspaceAuth) -> Self {
            Self {
                stored_auth: Arc::new(std::sync::Mutex::new(Some(auth))),
                token_valid: false,
            }
        }
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockInfra {
        async fn get_all_providers(&self) -> anyhow::Result<Vec<forge_domain::AnyProvider>> {
            Ok(vec![])
        }

        async fn get_provider(
            &self,
            _id: forge_domain::ProviderId,
        ) -> anyhow::Result<forge_domain::ProviderTemplate> {
            unimplemented!("Not needed for auth gate tests")
        }

        async fn upsert_credential(
            &self,
            _credential: forge_domain::AuthCredential,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_credential(
            &self,
            _id: &forge_domain::ProviderId,
        ) -> anyhow::Result<Option<forge_domain::AuthCredential>> {
            Ok(None)
        }

        async fn remove_credential(&self, _id: &forge_domain::ProviderId) -> anyhow::Result<()> {
            Ok(())
        }

        async fn migrate_env_credentials(
            &self,
        ) -> anyhow::Result<Option<forge_domain::MigrationResult>> {
            Ok(None)
        }

        async fn store_auth(&self, auth: &WorkspaceAuth) -> anyhow::Result<()> {
            *self.stored_auth.lock().unwrap() = Some(auth.clone());
            Ok(())
        }

        async fn get_auth(&self) -> anyhow::Result<Option<WorkspaceAuth>> {
            Ok(self.stored_auth.lock().unwrap().clone())
        }

        async fn clear_auth(&self) -> anyhow::Result<()> {
            *self.stored_auth.lock().unwrap() = None;
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl AuthFlowRepository for MockInfra {
        async fn init_flow(&self) -> anyhow::Result<InitFlowResponse> {
            unimplemented!()
        }

        async fn poll_auth(
            &self,
            _session_id: &str,
            _iv: &str,
            _aad: &str,
        ) -> anyhow::Result<Option<AuthFlowLoginInfo>> {
            unimplemented!()
        }

        async fn get_api_keys(&self, _token: &ApiKey) -> anyhow::Result<Vec<ApiKeyInfo>> {
            if self.token_valid {
                Ok(vec![])
            } else {
                Err(anyhow::anyhow!("Unauthenticated"))
            }
        }

        async fn delete_api_key(&self, _token: &ApiKey, _key_id: &str) -> anyhow::Result<()> {
            unimplemented!()
        }
    }

    fn auth_fixture() -> WorkspaceAuth {
        WorkspaceAuth {
            user_id: UserId::generate(),
            token: "test_token_abc123".to_string().into(),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_ensure_authenticated_with_valid_token() {
        let expected = auth_fixture();
        let fixture = AuthGateService::new(Arc::new(MockInfra::with_auth(expected.clone())));

        let actual = fixture.ensure_authenticated().await.unwrap();

        assert_eq!(actual.user_id, expected.user_id);
        assert_eq!(*actual.token, *expected.token);
    }

    #[tokio::test]
    async fn test_ensure_authenticated_with_invalid_token() {
        let auth = auth_fixture();
        let fixture = AuthGateService::new(Arc::new(MockInfra::with_invalid_token(auth)));

        let result = fixture.ensure_authenticated().await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Authentication required")
        );
    }

    #[tokio::test]
    async fn test_ensure_authenticated_without_stored_auth() {
        let fixture = AuthGateService::new(Arc::new(MockInfra::new()));

        let result = fixture.ensure_authenticated().await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Authentication required")
        );
    }

    #[tokio::test]
    async fn test_is_authenticated_returns_true_when_valid() {
        let auth = auth_fixture();
        let fixture = AuthGateService::new(Arc::new(MockInfra::with_auth(auth)));

        let actual = fixture.is_authenticated().await.unwrap();

        assert!(actual);
    }

    #[tokio::test]
    async fn test_is_authenticated_returns_false_when_invalid() {
        let auth = auth_fixture();
        let fixture = AuthGateService::new(Arc::new(MockInfra::with_invalid_token(auth)));

        let actual = fixture.is_authenticated().await.unwrap();

        assert!(!actual);
    }

    #[tokio::test]
    async fn test_is_authenticated_returns_false_when_no_auth() {
        let fixture = AuthGateService::new(Arc::new(MockInfra::new()));

        let actual = fixture.is_authenticated().await.unwrap();

        assert!(!actual);
    }

    #[tokio::test]
    async fn test_logout_clears_auth() {
        let auth = auth_fixture();
        let infra = Arc::new(MockInfra::with_auth(auth));
        let fixture = AuthGateService::new(infra.clone());

        fixture.logout().await.unwrap();
        let actual = infra.get_auth().await.unwrap();

        assert!(actual.is_none());
    }

    #[tokio::test]
    async fn test_get_current_auth_returns_stored_auth() {
        let expected = auth_fixture();
        let fixture = AuthGateService::new(Arc::new(MockInfra::with_auth(expected.clone())));

        let actual = fixture.get_current_auth().await.unwrap();

        assert!(actual.is_some());
        let actual = actual.unwrap();
        assert_eq!(actual.user_id, expected.user_id);
    }
}
