use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use forge_app::{AppConfig, ProviderRegistry};
use forge_domain::{Provider, ProviderUrl};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{EnvironmentInfra, HttpInfra};

type ProviderSearch = (&'static str, Box<dyn FnOnce(&str) -> Provider>);

#[derive(Deserialize, Serialize)]
#[serde(rename = "camelCase")]
struct AuthProviderResponse {
    auth_provider_id: String,
}

pub struct ForgeProviderRegistry<F> {
    infra: Arc<F>,
    // IMPORTANT: This cache is used to avoid logging out if the user has logged out from other
    // session. This helps to keep the user logged in for current session.
    cache: Arc<RwLock<Option<Provider>>>,
}

impl<F: EnvironmentInfra + HttpInfra> ForgeProviderRegistry<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, cache: Arc::new(Default::default()) }
    }

    fn provider_url(&self) -> Option<ProviderUrl> {
        if let Some(url) = self.infra.get_env_var("OPENAI_URL") {
            return Some(ProviderUrl::OpenAI(url));
        }

        // Check for Anthropic URL override
        if let Some(url) = self.infra.get_env_var("ANTHROPIC_URL") {
            return Some(ProviderUrl::Anthropic(url));
        }
        None
    }
    async fn get_provider(&self, forge_config: AppConfig) -> Option<Provider> {
        if let Some(forge_key) = &forge_config
            .key_info
            .as_ref()
            .and_then(|v| v.api_key.as_ref())
        {
            let provider = Provider::antinomy(forge_key.as_str());
            return Some(override_url(provider, self.provider_url()));
        }
        resolve_env_provider_with_tracking(
            self.provider_url(),
            self.infra.as_ref(),
            !forge_config.is_tracked,
        )
        .await
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + HttpInfra> ProviderRegistry for ForgeProviderRegistry<F> {
    async fn get_provider(&self, config: AppConfig) -> anyhow::Result<Provider> {
        if let Some(provider) = self.cache.read().await.as_ref() {
            return Ok(provider.clone());
        }

        let provider = self
            .get_provider(config)
            .await
            .context("Failed to detect upstream provider")?;
        self.cache.write().await.replace(provider.clone());
        Ok(provider)
    }
}

fn resolve_env_provider<F: EnvironmentInfra>(
    url: Option<ProviderUrl>,
    env: &F,
) -> Option<Provider> {
    let keys: [ProviderSearch; 6] = [
        ("FORGE_KEY", Box::new(Provider::antinomy)),
        ("OPENROUTER_API_KEY", Box::new(Provider::open_router)),
        ("REQUESTY_API_KEY", Box::new(Provider::requesty)),
        ("XAI_API_KEY", Box::new(Provider::xai)),
        ("OPENAI_API_KEY", Box::new(Provider::openai)),
        ("ANTHROPIC_API_KEY", Box::new(Provider::anthropic)),
    ];

    keys.into_iter().find_map(|(key, fun)| {
        env.get_env_var(key).map(|key| {
            let provider = fun(&key);
            override_url(provider, url.clone())
        })
    })
}

async fn resolve_env_provider_with_tracking<F: EnvironmentInfra + HttpInfra>(
    url: Option<ProviderUrl>,
    infra: &F,
    needs_track: bool,
) -> Option<Provider> {
    // Check for FORGE_KEY first to handle auth provider ID
    if needs_track {
        if let Some(forge_key) = infra.get_env_var("FORGE_KEY") {
            let mut provider = Provider::antinomy(&forge_key);
            provider = override_url(provider, url.clone());

            // Fetch auth provider ID for FORGE_KEY
            if let Some(key) = provider.key() {
                let user_url = format!("{}user", provider.to_base_url());
                let mut headers = HeaderMap::new();
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

                if let Ok(response) = infra
                    .post(
                        &user_url,
                        Bytes::from(
                            serde_json::to_string(&serde_json::json!({
                                "apiKey": key
                            }))
                            .unwrap_or_default(),
                        ),
                        Some(headers),
                    )
                    .await
                {
                    if let Ok(auth_response) = response.json::<AuthProviderResponse>().await {
                        provider.set_auth_provider_id(auth_response.auth_provider_id);
                    }
                }
            }

            return Some(provider);
        }
    }
    resolve_env_provider(url, infra)
}

fn override_url(mut provider: Provider, url: Option<ProviderUrl>) -> Provider {
    if let Some(url) = url {
        provider.url(url);
    }
    provider
}
