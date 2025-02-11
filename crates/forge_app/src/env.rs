use std::path::PathBuf;
use std::str::FromStr;

use forge_domain::{Environment, ProviderKind};

pub struct EnvironmentFactory {
    cwd: PathBuf,
    unrestricted: bool,
}

impl EnvironmentFactory {
    /// Creates a new EnvironmentFactory with current working directory
    ///
    /// # Arguments
    /// * `cwd` - The current working directory for the environment
    /// * `unrestricted` - If true, use unrestricted shell mode (sh/bash) If
    ///   false, use restricted shell mode (rbash)
    pub fn new(cwd: PathBuf, unrestricted: bool) -> Self {
        Self { cwd, unrestricted }
    }

    /// Get path to appropriate shell based on platform and mode
    fn get_shell_path(unrestricted: bool) -> String {
        if cfg!(target_os = "windows") {
            if unrestricted {
                std::env::var("COMSPEC").unwrap_or("cmd.exe".to_string())
            } else {
                // TODO: Add Windows restricted shell implementation
                std::env::var("COMSPEC").unwrap_or("cmd.exe".to_string())
            }
        } else if unrestricted {
            // Use user's preferred shell or fallback to sh
            std::env::var("SHELL").unwrap_or("/bin/sh".to_string())
        } else {
            // Default to rbash in restricted mode
            "/bin/rbash".to_string()
        }
    }

    pub fn create(&self) -> anyhow::Result<Environment> {
        dotenv::dotenv().ok();
        let cwd = self.cwd.clone();
        let large_model_id =
            std::env::var("FORGE_LARGE_MODEL").unwrap_or("anthropic/claude-3.5-sonnet".to_owned());
        let small_model_id =
            std::env::var("FORGE_SMALL_MODEL").unwrap_or("anthropic/claude-3.5-haiku".to_owned());

        let mut env = Environment {
            os: std::env::consts::OS.to_string(),
            cwd,
            shell: Self::get_shell_path(self.unrestricted),
            large_model_id,
            small_model_id,
            base_path: dirs::config_dir()
                .map(|a| a.join("forge"))
                .unwrap_or(PathBuf::from(".").join(".forge")),
            home: dirs::home_dir(),
            ..Default::default()
        };

        env = Self::api_key(env)?;
        env = Self::base_url(env)?;

        Ok(env)
    }

    fn api_key(mut env: Environment) -> anyhow::Result<Environment> {
        let provider = std::env::var("FORGE_PROVIDER")
            .ok()
            .and_then(|provider| ProviderKind::from_str(&provider).ok());
        let api_key = std::env::var("OPEN_ROUTER_KEY").ok();
        let provider = if api_key.is_some() {
            provider.unwrap_or(ProviderKind::OpenRouter)
        } else {
            provider.unwrap_or_default()
        };

        match (api_key, &provider) {
            (Some(api_key), _) => {
                env = env.api_key(api_key);
            }
            (None, ProviderKind::OpenRouter) => {
                return Err(anyhow::anyhow!("OpenRouter requires an API key"));
            }
            (_, _) => {}
        }

        env = env.provider_kind(provider);

        Ok(env)
    }

    fn base_url(mut env: Environment) -> anyhow::Result<Environment> {
        let base_url = std::env::var("FORGE_BASE_URL").ok();
        let provider = &env.provider_kind;

        match (base_url.as_ref(), provider) {
            (Some(base_url), _) => {
                env = env.base_url(base_url);
            }
            (None, ProviderKind::OpenRouter) => {
                env = env.base_url("https://openrouter.ai/api/v1/");
            }
            (None, ProviderKind::Ollama) => {
                env = env.base_url("http://localhost:11434/v1/");
            }
        }

        Ok(env)
    }
}
