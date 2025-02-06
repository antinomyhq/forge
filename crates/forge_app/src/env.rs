use std::path::PathBuf;
use std::str::FromStr;

use forge_domain::{Environment, ProviderKind};

pub struct EnvironmentFactory {
    cwd: PathBuf,
}

impl EnvironmentFactory {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    pub fn create(&self) -> anyhow::Result<Environment> {
        dotenv::dotenv().ok();
        let cwd = self.cwd.clone();
        let base_url = std::env::var("FORGE_BASE_URL").ok();
        let api_key = std::env::var("OPEN_ROUTER_KEY").ok();
        let large_model_id =
            std::env::var("FORGE_LARGE_MODEL").unwrap_or("anthropic/claude-3.5-sonnet".to_owned());
        let small_model_id =
            std::env::var("FORGE_SMALL_MODEL").unwrap_or("anthropic/claude-3.5-haiku".to_owned());
        let provider = std::env::var("FORGE_MODEL")
            .ok()
            .and_then(|provider| ProviderKind::from_str(&provider).ok());
        let provider = if api_key.is_some() {
            provider.unwrap_or(ProviderKind::OpenRouter)
        } else {
            provider.unwrap_or_default()
        };

        Ok(Environment {
            os: std::env::consts::OS.to_string(),
            cwd,
            shell: if cfg!(windows) {
                std::env::var("COMSPEC")?
            } else {
                std::env::var("SHELL").unwrap_or("/bin/sh".to_string())
            },
            api_key,
            large_model_id,
            small_model_id,
            base_path: dirs::config_dir()
                .map(|a| a.join("forge"))
                .unwrap_or(PathBuf::from(".").join(".forge")),
            home: dirs::home_dir(),
            base_url,
            provider,
        })
    }
}
