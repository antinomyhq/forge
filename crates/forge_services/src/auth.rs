use std::sync::Arc;

use anyhow::bail;
use bytes::Bytes;
use forge_app::{AuthService, EnvironmentInfra, Error, HttpInfra, User, UserUsage};
use forge_domain::{AppConfigRepository, InitAuth, LoginInfo};
use reqwest::Url;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};

const AUTH_ROUTE: &str = "auth/sessions/";
const USER_INFO_ROUTE: &str = "auth/user";
const USER_USAGE_ROUTE: &str = "auth/usage";

#[derive(Default, Clone)]
pub struct ForgeAuthService<I> {
    infra: Arc<I>,
}

impl<I: HttpInfra + EnvironmentInfra + AppConfigRepository> ForgeAuthService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
    async fn init(&self) -> anyhow::Result<InitAuth> {
        let init_url = format!("{}{AUTH_ROUTE}", self.infra.get_environment().forge_api_url);
        let init_url = Url::parse(&init_url)?;
        let resp = self.infra.http_post(&init_url, None, Bytes::new()).await?;
        if !resp.status().is_success() {
            bail!("Failed to initialize auth")
        }

        Ok(serde_json::from_slice(&resp.bytes().await?)?)
    }

    async fn login(&self, auth: &InitAuth) -> anyhow::Result<LoginInfo> {
        let url = format!(
            "{}{AUTH_ROUTE}{}",
            self.infra.get_environment().forge_api_url,
            auth.session_id
        );
        let url = Url::parse(&url)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", auth.token))?,
        );

        let response = self.infra.http_get(&url, Some(headers)).await?;
        match response.status().as_u16() {
            200 => Ok(serde_json::from_slice::<LoginInfo>(
                &response.bytes().await?,
            )?),
            202 => Err(Error::AuthInProgress.into()),
            status => bail!("HTTP {status}: Authentication failed"),
        }
    }

    async fn user_info(&self, api_key: &str) -> anyhow::Result<User> {
        let url = format!(
            "{}{USER_INFO_ROUTE}",
            self.infra.get_environment().forge_api_url
        );

        let url = Url::parse(&url)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))?,
        );

        let response = self
            .infra
            .http_get(&url, Some(headers))
            .await?
            .error_for_status()?;

        Ok(serde_json::from_slice(&response.bytes().await?)?)
    }

    async fn user_usage(&self, api_key: &str) -> anyhow::Result<UserUsage> {
        let url = Url::parse(&format!(
            "{}{USER_USAGE_ROUTE}",
            self.infra.get_environment().forge_api_url
        ))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))?,
        );

        let response = self
            .infra
            .http_get(&url, Some(headers))
            .await?
            .error_for_status()?;

        Ok(serde_json::from_slice(&response.bytes().await?)?)
    }

    async fn get_auth_token(&self) -> anyhow::Result<Option<LoginInfo>> {
        let config = self.infra.get_app_config().await?;
        Ok(config.key_info)
    }

    async fn set_auth_token(&self, login: Option<LoginInfo>) -> anyhow::Result<()> {
        let mut config = self.infra.get_app_config().await?;
        config.key_info = login;
        self.infra.set_app_config(&config).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<I: HttpInfra + EnvironmentInfra + AppConfigRepository> AuthService for ForgeAuthService<I> {
    async fn init_auth(&self) -> anyhow::Result<InitAuth> {
        self.init().await
    }

    async fn login(&self, auth: &InitAuth) -> anyhow::Result<LoginInfo> {
        self.login(auth).await
    }

    async fn user_info(&self, api_key: &str) -> anyhow::Result<User> {
        self.user_info(api_key).await
    }

    async fn user_usage(&self, api_key: &str) -> anyhow::Result<UserUsage> {
        self.user_usage(api_key).await
    }

    async fn get_auth_token(&self) -> anyhow::Result<Option<LoginInfo>> {
        self.get_auth_token().await
    }

    async fn set_auth_token(&self, token: Option<LoginInfo>) -> anyhow::Result<()> {
        self.set_auth_token(token).await
    }
}
