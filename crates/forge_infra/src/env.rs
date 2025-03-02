use std::path::PathBuf;

use forge_app::EnvironmentService;
use forge_domain::{Environment, Provider};

pub struct ForgeEnvironmentService {
    restricted: bool,
}

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

    fn get(&self) -> Environment {
        dotenv::dotenv().ok();
        let cwd = std::env::current_dir().unwrap_or(PathBuf::from("."));

        let provider_key = std::env::var("FORGE_KEY")
            .or_else(|_| std::env::var("OPENROUTER_API_KEY"))
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
            .expect("No API key found. Please set one of: FORGE_KEY, OPENROUTER_API_KEY, OPENAI_API_KEY or ANTHROPIC_API_KEY");
        // note: since we know the key is set, we can unwrap here.
        let provider = Provider::from_env().unwrap();
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
            provider_key,
            provider_url: provider.to_base_url().to_string(),
            openai_key: std::env::var("OPENAI_API_KEY").ok(),
        }
    }
}

impl EnvironmentService for ForgeEnvironmentService {
    fn get_environment(&self) -> Environment {
        self.get()
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use forge_domain::Provider;
    use serial_test::serial;

    // reset the env variables for reliable tests
    fn reset_env() {
        env::remove_var("FORGE_KEY");
        env::remove_var("FORGE_PROVIDER_URL");
        env::remove_var("OPENROUTER_API_KEY");
        env::remove_var("OPENAI_API_KEY");
        env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    #[serial]
    fn test_provider_from_env_with_forge_key_and_without_provider_url() {
        reset_env();
        env::set_var("FORGE_KEY", "some_forge_key");

        let provider = Provider::from_env();
        assert_eq!(provider, None);
    }

    #[test]
    #[serial]
    fn test_provider_from_env_with_forge_key() {
        reset_env();
        env::set_var("FORGE_KEY", "some_forge_key");
        env::set_var("FORGE_PROVIDER_URL", "https://api.openai.com/v1/");

        let provider = Provider::from_env();
        assert_eq!(provider, Some(Provider::OpenAI));
    }

    #[test]
    #[serial]
    fn test_provider_from_env_with_open_router_key() {
        reset_env();
        env::set_var("OPENROUTER_API_KEY", "some_open_router_key");

        let provider = Provider::from_env();
        assert_eq!(provider, Some(Provider::OpenRouter));
    }

    #[test]
    #[serial]
    fn test_provider_from_env_with_openai_key() {
        reset_env();
        env::set_var("OPENAI_API_KEY", "some_openai_key");

        let provider = Provider::from_env();
        assert_eq!(provider, Some(Provider::OpenAI));
    }

    #[test]
    #[serial]
    fn test_provider_from_env_with_anthropic_key() {
        reset_env();
        env::set_var("ANTHROPIC_API_KEY", "some_anthropic_key");

        let provider = Provider::from_env();
        assert_eq!(provider, Some(Provider::Anthropic));
    }

    #[test]
    #[serial]
    fn test_provider_from_env_with_no_keys() {
        reset_env();
        let provider = Provider::from_env();
        assert_eq!(provider, None);
    }

    #[test]
    #[serial]
    fn test_from_url() {
        assert_eq!(
            Provider::from_url("https://api.openai.com/v1/"),
            Some(Provider::OpenAI)
        );
        assert_eq!(
            Provider::from_url("https://api.openrouter.io/v1/"),
            Some(Provider::OpenRouter)
        );
        assert_eq!(
            Provider::from_url("https://api.anthropic.com/v1/"),
            Some(Provider::Anthropic)
        );
        assert_eq!(Provider::from_url("https://unknown.url/"), None);
    }

    #[test]
    #[serial]
    fn test_to_url() {
        assert_eq!(Provider::OpenAI.to_base_url(), "https://api.openai.com/v1/");
        assert_eq!(
            Provider::OpenRouter.to_base_url(),
            "https://api.openrouter.io/v1/"
        );
        assert_eq!(
            Provider::Anthropic.to_base_url(),
            "https://api.anthropic.com/v1/"
        );
    }
}
