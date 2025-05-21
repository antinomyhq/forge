use std::sync::Arc;

use forge_domain::{AuthService, InitAuth, KeyService, Provider, RetryConfig};

use crate::{HttpService, Infrastructure};

#[derive(Default, Clone)]
pub struct ForgeAuthService<I, K> {
    infra: Arc<I>,
    key_service: Arc<K>,
}

impl<I: Infrastructure, K: KeyService> ForgeAuthService<I, K> {
    pub fn new(infra: Arc<I>, key_service: Arc<K>) -> Self {
        Self { infra, key_service }
    }
    async fn init(&self) -> anyhow::Result<InitAuth> {
        let init_url = format!("{}cli/auth/init", Provider::ANTINOMY_URL);
        let resp = self.infra.http_service().get(&init_url).await?;

        Ok(serde_json::from_slice(&resp.body)?)
    }

    async fn login(&self, auth: &InitAuth) -> anyhow::Result<()> {
        let url = format!(
            "{}cli/auth/token/{}",
            Provider::ANTINOMY_URL,
            auth.session_id
        );

        // FIXME: we should call cli/auth/token/{session_id} to get the token
        // and send the token back to cli/auth/complete/{session_id} and expect
        // `ForgeKey` in response. NOTE that this needs change in the backend.

        let response = self.infra.http_service().get(&url).await?;
        match response.status.as_u16() {
            200 => {
                self.key_service
                    .set(serde_json::from_slice(&response.body)?)
                    .await
            }
            202 => anyhow::bail!("Login timeout"),
            _ => anyhow::bail!("Failed to log in"),
        }
    }
    async fn logout(&self) -> anyhow::Result<()> {
        self.key_service.delete().await
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure, C: KeyService> AuthService for ForgeAuthService<I, C> {
    async fn init(&self) -> anyhow::Result<InitAuth> {
        self.init().await
    }

    async fn login(&self, auth: &InitAuth) -> anyhow::Result<()> {
        self.infra
            .http_service()
            // TODO: Add `when` config to differentiate b/w 202 and other error codes.
            .poll(
                RetryConfig::default()
                    .max_retry_attempts(300usize)
                    .max_delay(2)
                    .backoff_factor(1u64),
                || self.login(auth),
            )
            .await
    }

    async fn logout(&self) -> anyhow::Result<()> {
        self.logout().await
    }
}
