use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use forge_domain::{InitAuth, RetryConfig};

use crate::{AuthService, Error};

pub struct Authenticator<S> {
    service: Arc<S>,
    config: RetryConfig,
}

impl<S: AuthService> Authenticator<S> {
    pub fn new(service: Arc<S>, config: RetryConfig) -> Self {
        Self { service, config }
    }
    pub async fn init(&self) -> anyhow::Result<InitAuth> {
        self.service.init_auth().await
    }
    pub async fn login(&self, init_auth: &InitAuth) -> anyhow::Result<()> {
        self.poll(self.config.clone(), || self.login_inner(init_auth))
            .await
    }
    pub async fn logout(&self) -> anyhow::Result<()> {
        self.service.set_auth_token(None).await?;
        Ok(())
    }
    async fn login_inner(&self, init_auth: &InitAuth) -> anyhow::Result<()> {
        let key_info = self.service.get_auth_token().await?;
        if key_info.is_some() {
            return Ok(());
        }
        let key = self.service.login(init_auth).await?;
        self.service.set_auth_token(Some(key)).await?;
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
