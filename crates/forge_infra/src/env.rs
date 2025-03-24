use std::path::PathBuf;

use forge_domain::{Environment, Provider};
use forge_services::EnvironmentService;

pub struct ForgeEnvironmentService {
    restricted: bool,
}

type ProviderSearch = (&'static str, Box<dyn FnOnce(&str) -> Provider>);

impl ForgeEnvironmentService {
    /// Creates a new EnvironmentFactory with current working directory
    ///
    /// # Arguments
    /// * `unrestricted` - If true, use unrestricted shell mode (sh/bash) If
    ///   false, use restricted shell mode (rbash)
    pub fn new(restricted: bool) -> Self {
        Self { restricted }
    }

    /// Get path to appropriate shell based on platform and mode
    fn get_shell_path(&self) -> String {
        if cfg!(target_os = "windows") {
            std::env::var("COMSPEC").unwrap_or("cmd.exe".to_string())
        } else if self.restricted {
            // Default to rbash in restricted mode
            "/bin/rbash".to_string()
        } else {
            // Use user's preferred shell or fallback to sh
            std::env::var("SHELL").unwrap_or("/bin/sh".to_string())
        }
    }

    /// Resolves the provider key and provider from environment variables
    ///
    /// Returns a tuple of (provider_key, provider)
    /// Panics if no API key is found in the environment
    fn resolve_provider(&self) -> Provider {
        let keys: [ProviderSearch; 4] = [
            ("FORGE_KEY", Box::new(Provider::antinomy)),
            ("OPENROUTER_API_KEY", Box::new(Provider::open_router)),
            ("OPENAI_API_KEY", Box::new(Provider::openai)),
            ("ANTHROPIC_API_KEY", Box::new(Provider::anthropic)),
        ];

        let env_variables = keys
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>()
            .join(", ");

        keys.into_iter()
            .find_map(|(key, fun)| {
                std::env::var(key).ok().map(|key| {
                    let mut provider = fun(&key);

                    if let Ok(url) = std::env::var("OPENAI_URL") {
                        provider.open_ai_url(url);
                    }

                    // Check for Anthropic URL override
                    if let Ok(url) = std::env::var("ANTHROPIC_URL") {
                        provider.anthropic_url(url);
                    }

                    provider
                })
            })
            .unwrap_or_else(|| panic!("No API key found. Please set one of: {}", env_variables))
    }

    fn get(&self) -> Environment {
        dotenv::dotenv().ok();
        let cwd = std::env::current_dir().unwrap_or(PathBuf::from("."));
        let provider = self.resolve_provider();

        Environment {
            os: std::env::consts::OS.to_string(),
            pid: std::process::id(),
            cwd,
            shell: self.get_shell_path(),
            base_path: dirs::config_dir()
                .map(|a| a.join("forge"))
                .unwrap_or(PathBuf::from(".").join(".forge")),
            home: dirs::home_dir(),

            qdrant_key: std::env::var("QDRANT_KEY").ok(),
            qdrant_cluster: std::env::var("QDRANT_CLUSTER").ok(),
            openai_key: std::env::var("OPENAI_API_KEY").ok(),
            provider,
        }
    }
}

impl EnvironmentService for ForgeEnvironmentService {
    fn get_environment(&self) -> Environment {
        self.get()
    }
}
