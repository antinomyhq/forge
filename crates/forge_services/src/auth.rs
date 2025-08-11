use std::sync::Arc;

use anyhow::bail;
use bytes::Bytes;
use forge_app::dto::{InitAuth, LoginInfo};
use forge_app::{AuthService, Error, User, UserUsage};
use reqwest::Url;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};

use crate::EnvironmentInfra;
use crate::infra::HttpInfra;

const AUTH_ROUTE: &str = "auth/sessions/";
const USER_INFO_ROUTE: &str = "auth/user";
const USER_USAGE_ROUTE: &str = "auth/usage";

#[derive(Default, Clone)]
pub struct ForgeAuthService<I> {
    infra: Arc<I>,
}

impl<I: HttpInfra + EnvironmentInfra> ForgeAuthService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
    async fn init(&self) -> anyhow::Result<InitAuth> {
        let init_url = format!("{}{AUTH_ROUTE}", self.infra.get_environment().forge_api_url);
        let init_url = Url::parse(&init_url)?;
        let resp = self.infra.post(&init_url, Bytes::new()).await?;
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

        let response = self.infra.get(&url, Some(headers)).await?;
        match response.status().as_u16() {
            200 => Ok(serde_json::from_slice::<LoginInfo>(
                &response.bytes().await?,
            )?),
            202 => Err(Error::AuthInProgress.into()),
            status => bail!("HTTP {}: Authentication failed", status),
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
            .get(&url, Some(headers))
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
            .get(&url, Some(headers))
            .await?
            .error_for_status()?;

        Ok(serde_json::from_slice(&response.bytes().await?)?)
    }
}

#[async_trait::async_trait]
impl<I: HttpInfra + EnvironmentInfra> AuthService for ForgeAuthService<I> {
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
}
