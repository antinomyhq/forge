use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use forge_domain::RetryConfig;

use crate::dto::InitAuth;
use crate::{AuthService, Error};

/// Platform authenticator for Forge API access
///
/// Handles Forge platform authentication using device code flow:
/// 1. **init()** - Initiate device authorization and get user code
/// 2. **login()** - Poll until user completes authorization (with retry)
/// 3. **logout()** - Clear stored credentials
///
/// For LLM provider authentication, use `ProviderAuthService` directly via
/// `ForgeApp::provider_auth()`.
pub struct Authenticator<S> {
    auth_service: Arc<S>,
}

impl<S: AuthService> Authenticator<S> {
    /// Creates a new platform authenticator
    pub fn new(auth_service: Arc<S>) -> Self {
        Self { auth_service }
    }

    /// Initializes Forge platform authentication
    ///
    /// Returns device code information for user to authorize in browser
    pub async fn init(&self) -> anyhow::Result<InitAuth> {
        self.auth_service.init_auth().await
    }

    /// Polls until user completes Forge platform authentication
    ///
    /// This blocks until the user authorizes the device code in their browser
    /// or the timeout is reached.
    pub async fn login(&self, init_auth: &InitAuth) -> anyhow::Result<()> {
        self.poll(
            RetryConfig::default()
                .max_retry_attempts(300usize)
                .max_delay(2)
                .backoff_factor(1u64),
            || self.login_inner(init_auth),
        )
        .await
    }

    /// Logs out of Forge platform by clearing stored credentials
    pub async fn logout(&self) -> anyhow::Result<()> {
        self.auth_service.set_auth_token(None).await?;
        Ok(())
    }

    async fn login_inner(&self, init_auth: &InitAuth) -> anyhow::Result<()> {
        let key_info = self.auth_service.get_auth_token().await?;
        if key_info.is_some() {
            return Ok(());
        }
        let key = self.auth_service.login(init_auth).await?;
        self.auth_service.set_auth_token(Some(key)).await?;
        Ok(())
    }

    async fn poll<T, F>(
        &self,
        config: RetryConfig,
        call: impl Fn() -> F + Send,
    ) -> anyhow::Result<T>
    where
        F: Future<Output = anyhow::Result<T>> + Send,
    {
        let mut builder = ExponentialBuilder::default()
            .with_factor(1.0)
            .with_factor(config.backoff_factor as f32)
            .with_max_times(config.max_retry_attempts)
            .with_jitter();
        if let Some(max_delay) = config.max_delay {
            builder = builder.with_max_delay(Duration::from_secs(max_delay))
        }

        call.retry(builder)
            .when(|e| {
                // Only retry on Error::AuthInProgress (202 status)
                e.downcast_ref::<Error>()
                    .map(|v| matches!(v, Error::AuthInProgress))
                    .unwrap_or(false)
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_poll_retry_condition() {
        // Test that the retry condition only matches AuthInProgress errors
        let auth_in_progress_error = anyhow::Error::from(Error::AuthInProgress);
        let other_error = anyhow::anyhow!("Some other error");
        let serde_error = anyhow::Error::from(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "test",
        )));

        // Create a test closure that mimics the retry condition
        let retry_condition = |e: &anyhow::Error| {
            if let Some(app_error) = e.downcast_ref::<Error>() {
                matches!(app_error, Error::AuthInProgress)
            } else {
                false
            }
        };

        // Test cases
        assert_eq!(retry_condition(&auth_in_progress_error), true);
        assert_eq!(retry_condition(&other_error), false);
        assert_eq!(retry_condition(&serde_error), false);
    }
}
