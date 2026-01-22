use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use forge_domain::{
    ApiKey, AuthFlowLoginInfo, AuthFlowRepository, InitFlowResponse, UserId, WorkspaceAuth,
};
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Authentication service that orchestrates the device-based auth flow
pub struct AuthFlowService<R> {
    repository: Arc<R>,
}

impl<R> AuthFlowService<R> {
    /// Create a new authentication service
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

/// Session data stored during polling
struct SessionData {
    session_id: String,
    iv: String,
    aad: String,
    expires_at: Instant,
}

impl<R: AuthFlowRepository> AuthFlowService<R> {
    /// Execute complete authentication flow
    ///
    /// # Arguments
    /// * `poll_interval` - How often to poll (in seconds)
    ///
    /// # Errors
    /// Returns an error if authentication fails or times out
    pub async fn authenticate(&self, poll_interval_secs: u64) -> Result<WorkspaceAuth> {
        // 1. Initialize the flow
        info!("Initializing authentication flow");
        let init_response = self
            .repository
            .init_flow()
            .await
            .context("Failed to initialize authentication flow")?;

        debug!(
            "Auth flow initialized: device_id={}, ttl={}s",
            init_response.device_id, init_response.ttl
        );

        // 2. Store session data for polling
        let session_data = SessionData {
            session_id: init_response.device_id.clone(),
            iv: init_response.iv.clone(),
            aad: init_response.aad.clone(),
            expires_at: Instant::now() + Duration::from_secs(init_response.ttl),
        };

        // 3. Return device code and TTL for UI display
        // (This will be handled by the caller displaying the UI)

        // 4. Poll until completion or timeout
        let login_info = self
            .poll_until_complete(session_data, poll_interval_secs)
            .await?;

        info!("Authentication successful");

        // 5. Extract user_id from token (decode JWT or use server info)
        // For now, we'll generate a user_id since the token doesn't expose it
        // In production, this would be extracted from the JWT or fetched from server
        let user_id = self.extract_user_id_from_token(&login_info.token).await?;

        Ok(WorkspaceAuth { user_id, token: login_info.token, created_at: Utc::now() })
    }

    /// Poll for authentication completion
    ///
    /// # Arguments
    /// * `session` - Session data from init_flow
    /// * `poll_interval_secs` - How often to poll
    ///
    /// # Errors
    /// Returns an error if polling fails or times out
    async fn poll_until_complete(
        &self,
        session: SessionData,
        poll_interval_secs: u64,
    ) -> Result<AuthFlowLoginInfo> {
        let poll_interval = Duration::from_secs(poll_interval_secs);

        loop {
            // Check if session expired
            if Instant::now() >= session.expires_at {
                warn!("Authentication session expired");
                anyhow::bail!(AuthError::Timeout);
            }

            // Poll the server
            debug!("Polling for authentication completion");
            match self
                .repository
                .poll_auth(&session.session_id, &session.iv, &session.aad)
                .await
            {
                Ok(Some(login_info)) => {
                    info!("Authentication completed successfully");
                    return Ok(login_info);
                }
                Ok(None) => {
                    // Still pending, wait and retry
                    debug!(
                        "Authentication still pending, waiting {}s",
                        poll_interval_secs
                    );
                    sleep(poll_interval).await;
                }
                Err(e) => {
                    warn!("Error polling for authentication: {}", e);
                    return Err(e);
                }
            }
        }
    }

    /// Extract user ID from authentication token
    ///
    /// # Arguments
    /// * `token` - Authentication token
    ///
    /// # Errors
    /// Returns an error if user ID cannot be extracted
    async fn extract_user_id_from_token(&self, _token: &ApiKey) -> Result<UserId> {
        // TODO: In production, decode JWT to extract user_id
        // For now, we'll generate a new user ID
        // The server should ideally return user_id in the LoginInfo
        Ok(UserId::generate())
    }

    /// Check if existing token is still valid
    ///
    /// # Arguments
    /// * `token` - Authentication token to validate
    ///
    /// # Errors
    /// Returns an error if validation check fails (network error)
    pub async fn validate_token(&self, token: &ApiKey) -> Result<bool> {
        debug!("Validating authentication token");

        match self.repository.get_api_keys(token).await {
            Ok(_) => {
                debug!("Token is valid");
                Ok(true)
            }
            Err(e) => {
                // Check if it's an auth error (401/403) vs network error
                if is_auth_error(&e) {
                    debug!("Token is invalid or expired");
                    Ok(false)
                } else {
                    // Network or other error, propagate it
                    warn!("Error validating token: {}", e);
                    Err(e)
                }
            }
        }
    }

    /// Get the init flow response for UI display
    ///
    /// This is a convenience method for getting just the init response
    /// without starting the full authentication flow
    ///
    /// # Errors
    /// Returns an error if initialization fails
    pub async fn init_flow(&self) -> Result<InitFlowResponse> {
        self.repository.init_flow().await
    }
}

/// Check if an error is an authentication error (401/403)
// FIXME: need better approach
fn is_auth_error(error: &anyhow::Error) -> bool {
    // Check error message for common auth error patterns
    let error_msg = error.to_string().to_lowercase();
    error_msg.contains("unauthenticated")
        || error_msg.contains("unauthorized")
        || error_msg.contains("permission denied")
        || error_msg.contains("invalid token")
        || error_msg.contains("expired token")
        || error_msg.contains("401")
        || error_msg.contains("403")
}

/// Authentication errors
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Authentication timeout - device code expired
    #[error("Authentication timeout - device code expired")]
    Timeout,

    /// Authentication cancelled by user
    #[error("Authentication cancelled by user")]
    Cancelled,

    /// Invalid or expired token
    #[error("Invalid or expired token")]
    InvalidToken,
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use forge_domain::{ApiKeyInfo, AuthFlowLoginInfo, InitFlowResponse};

    use super::*;

    struct MockAuthFlowRepository {
        init_response: InitFlowResponse,
        poll_responses: Mutex<Vec<Option<AuthFlowLoginInfo>>>,
        validate_result: bool,
    }

    #[async_trait::async_trait]
    impl AuthFlowRepository for MockAuthFlowRepository {
        async fn init_flow(&self) -> Result<InitFlowResponse> {
            Ok(self.init_response.clone())
        }

        async fn poll_auth(
            &self,
            _session_id: &str,
            _iv: &str,
            _aad: &str,
        ) -> Result<Option<AuthFlowLoginInfo>> {
            let mut responses = self.poll_responses.lock().unwrap();
            Ok(responses.pop().unwrap_or(None))
        }

        async fn get_api_keys(&self, _token: &ApiKey) -> Result<Vec<ApiKeyInfo>> {
            if self.validate_result {
                Ok(vec![])
            } else {
                Err(anyhow::anyhow!("Unauthenticated: Invalid token"))
            }
        }

        async fn delete_api_key(&self, _token: &ApiKey, _key_id: &str) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_validate_token_valid() {
        let repo = MockAuthFlowRepository {
            init_response: InitFlowResponse {
                device_id: "ABC123".to_string(),
                ttl: 300,
                iv: "test_iv".to_string(),
                aad: "test_aad".to_string(),
            },
            poll_responses: Mutex::new(vec![]),
            validate_result: true,
        };

        let service = AuthFlowService::new(Arc::new(repo));
        let token: ApiKey = "test_token".to_string().into();

        let actual = service.validate_token(&token).await.unwrap();

        assert!(actual);
    }

    #[tokio::test]
    async fn test_validate_token_invalid() {
        let repo = MockAuthFlowRepository {
            init_response: InitFlowResponse {
                device_id: "ABC123".to_string(),
                ttl: 300,
                iv: "test_iv".to_string(),
                aad: "test_aad".to_string(),
            },
            poll_responses: Mutex::new(vec![]),
            validate_result: false,
        };

        let service = AuthFlowService::new(Arc::new(repo));
        let token: ApiKey = "invalid_token".to_string().into();

        let actual = service.validate_token(&token).await.unwrap();

        assert!(!actual);
    }
}
