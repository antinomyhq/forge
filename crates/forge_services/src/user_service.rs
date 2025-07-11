use std::sync::Arc;

use bytes::Bytes;
use forge_app::UserService;
use forge_domain::{Provider, User};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::HttpInfra;

pub struct ForgeUserService<I> {
    infra: Arc<I>,
    cache: Arc<Mutex<Option<User>>>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename = "camelCase")]
struct AuthProviderResponse {
    auth_provider_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename = "camelCase")]
struct AuthProviderRequest {
    api_key: String,
}

impl<I: HttpInfra> ForgeUserService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra, cache: Arc::new(Default::default()) }
    }
    async fn fetch_user(&self, provider: Provider) -> anyhow::Result<User> {
        if provider.is_antinomy() {
            if let Some(key) = provider.key() {
                let user_url = format!("{}user", provider.to_base_url());
                let mut headers = HeaderMap::new();
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

                if let Ok(response) = self
                    .infra
                    .post(
                        &user_url,
                        Bytes::from(
                            serde_json::to_string(&AuthProviderRequest {
                                api_key: key.to_string(),
                            })
                            .unwrap_or_default(),
                        ),
                        Some(headers),
                    )
                    .await
                {
                    if let Ok(auth_response) = response.json::<AuthProviderResponse>().await {
                        return Ok(User {
                            auth_provider_id: Some(auth_response.auth_provider_id),
                            provider,
                            is_tracked: false,
                        });
                    }
                }
            }
        }

        Ok(User { auth_provider_id: None, provider, is_tracked: false })
    }
}

#[async_trait::async_trait]
impl<I: HttpInfra> UserService for ForgeUserService<I> {
    async fn fetch_user(&self, provider: Provider) -> anyhow::Result<User> {
        if let Some(cached_user) = self.cache.lock().await.as_ref() {
            if cached_user.provider == provider {
                return Ok(User { is_tracked: true, ..cached_user.clone() });
            }
        }
        self.fetch_user(provider).await
    }
}
