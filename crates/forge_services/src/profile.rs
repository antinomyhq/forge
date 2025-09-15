use std::sync::Arc;
use std::{fs, io};

use anyhow::Context;
use forge_app::ProfileService;
use forge_app::domain::{Provider, ProviderUrl};
use forge_app::dto::{Profile, ProfileName};
use serde::{Deserialize, Serialize};

use crate::EnvironmentInfra;

#[derive(Serialize, Deserialize, Clone)]
pub struct ProviderConfig {
    provider: String,
    api_key: Option<String>,
    base_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProfileYaml {
    pub config: ProviderConfig,
    #[serde(flatten)]
    pub profile: Profile,
}

#[derive(Serialize, Deserialize, Clone)]
struct ProfilesFile {
    active_profile: Option<ProfileName>,
    profiles: Vec<ProfileYaml>,
}

pub struct ForgeProfileService<F: EnvironmentInfra> {
    infra: Arc<F>,
}

impl<F: EnvironmentInfra> ForgeProfileService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    fn load_yaml(&self) -> anyhow::Result<ProfilesFile> {
        let profile_path = self.infra.get_environment().base_path.join("profiles.yaml");
        const DEFAULT_CONFIG: &str = include_str!("../../../profiles.default.yaml");

        let create_and_load_default = || {
            fs::write(&profile_path, DEFAULT_CONFIG).with_context(|| {
                format!(
                    "Failed to write default config to {}",
                    profile_path.display()
                )
            })?;

            serde_yml::from_str(DEFAULT_CONFIG).map_err(Into::into)
        };

        match fs::read_to_string(&profile_path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    create_and_load_default()
                } else {
                    serde_yml::from_str(&content).with_context(|| {
                        format!("Invalid file format in {}.", profile_path.display())
                    })
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => create_and_load_default(),
            Err(err) => {
                Err(err).with_context(|| format!("Failed to read {}", profile_path.display()))
            }
        }
    }

    fn config_to_profile(&self, def: &ProfileYaml) -> Profile {
        let mut profile = def.profile.clone();
        profile.provider = self.config_to_provider(&def.config).unwrap_or_default();
        profile
    }

    fn config_to_provider(&self, def: &ProviderConfig) -> Option<Provider> {
        let api_key = self.resolve_api_key(def)?;
        let mut provider = match def.provider.as_str() {
            "openai" => Provider::openai(&api_key),
            "anthropic" => Provider::anthropic(&api_key),
            "xai" => Provider::xai(&api_key),
            "openrouter" => Provider::open_router(&api_key),
            "requesty" => Provider::requesty(&api_key),
            "zai" => Provider::zai(&api_key),
            "cerebras" => Provider::cerebras(&api_key),
            _ => return None,
        };

        if let Some(base_url) = &def.base_url {
            let url = match def.provider.as_str() {
                "openai" | "xai" | "openrouter" | "requesty" | "zai" | "cerebras" => {
                    ProviderUrl::OpenAI(base_url.clone())
                }
                "anthropic" => ProviderUrl::Anthropic(base_url.clone()),
                _ => return None,
            };
            provider.url(url);
        }

        Some(provider)
    }

    fn resolve_api_key(&self, def: &ProviderConfig) -> Option<String> {
        if let Some(key) = &def.api_key {
            return Some(key.clone());
        }

        let env_var = match def.provider.as_str() {
            "openai" => "OPENAI_API_KEY",
            "anthropic" => "ANTHROPIC_API_KEY",
            "xai" => "XAI_API_KEY",
            "openrouter" => "OPENROUTER_API_KEY",
            "requesty" => "REQUESTY_API_KEY",
            "zai" => "ZAI_API_KEY",
            "cerebras" => "CEREBRAS_API_KEY",
            _ => return None,
        };

        self.infra.get_env_var(env_var)
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra> ProfileService for ForgeProfileService<F> {
    async fn get_active_profile(&self) -> anyhow::Result<Option<Profile>> {
        let profiles_file = self.load_yaml()?;
        if let Some(profile_name) = profiles_file.active_profile {
            let profiles = self.list_profiles().await?;
            Ok(profiles.into_iter().find(|p| p.name == profile_name))
        } else {
            Ok(None)
        }
    }

    async fn list_profiles(&self) -> anyhow::Result<Vec<Profile>> {
        let profiles_file = self.load_yaml()?;
        let profile_list = profiles_file
            .profiles
            .iter()
            .map(|def| self.config_to_profile(def))
            .collect();
        Ok(profile_list)
    }

    async fn set_active_profile(&self, profile_name: ProfileName) -> anyhow::Result<()> {
        let profile_path = self.infra.get_environment().base_path.join("profiles.yaml");
        let mut profiles_file = self.load_yaml()?;
        profiles_file.active_profile = Some(profile_name);
        let content = serde_yml::to_string(&profiles_file)?;
        fs::write(profile_path, content)?;
        Ok(())
    }
}
