use anyhow::bail;
use forge_domain::{AuthService, InitAuth, KeyService, Provider, ProviderUrl, RetryConfig};
use std::sync::Arc;

use crate::{HttpService, Infrastructure, ProviderService};

const TOKEN_POLL_ROUTE: &str = "cli/auth/token/";
const AUTH_INIT_ROUTE: &str = "cli/auth/init";
const AUTH_CANCEL_ROUTE: &str = "cli/auth/cancel/";

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
        let init_url = format!(
            "{}{AUTH_INIT_ROUTE}",
            self.infra
                .provider_service()
                .provider_url()
                .map(ProviderUrl::into_string)
                .unwrap_or(Provider::ANTINOMY_URL.to_string())
        );
        let resp = self.infra.http_service().get(&init_url).await?;
        if !resp.status.is_success() {
            bail!("Failed to initialize auth")
        }

        Ok(serde_json::from_slice(&resp.body)?)
    }

    async fn login(&self, auth: &InitAuth) -> anyhow::Result<()> {
        if self.key_service.get().await.is_some() {
            let url = format!(
                "{}{AUTH_CANCEL_ROUTE}{}",
                self.infra
                    .provider_service()
                    .provider_url()
                    .map(ProviderUrl::into_string)
                    .unwrap_or(Provider::ANTINOMY_URL.to_string()),
                auth.session_id,
            );

            // Delete the session if auth is already completed in another session.
            self.infra.http_service().delete(&url).await?;
            return Ok(());
        }
        let url = format!(
            "{}{TOKEN_POLL_ROUTE}{}",
            self.infra
                .provider_service()
                .provider_url()
                .map(ProviderUrl::into_string)
                .unwrap_or(Provider::ANTINOMY_URL.to_string()),
            auth.session_id
        );

        let response = self.infra.http_service().get(&url).await?;
        match response.status.as_u16() {
            200 => {
                self.key_service
                    .set(serde_json::from_slice(&response.body)?)
                    .await
            }
            202 => bail!("Login timeout"),
            _ => bail!("Failed to log in"),
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
