use std::path::{Path, PathBuf};
use std::sync::RwLock;

use forge_domain::{Environment, Provider, RetryConfig};
use forge_services::EnvironmentService;

pub struct ForgeEnvironmentService {
    restricted: bool,
    is_env_loaded: RwLock<bool>,
}

type ProviderSearch = (&'static str, Box<dyn FnOnce(&str) -> Provider>);

impl ForgeEnvironmentService {
    /// Creates a new EnvironmentFactory with current working directory
    ///
    /// # Arguments
    /// * `unrestricted` - If true, use unrestricted shell mode (sh/bash) If
    ///   false, use restricted shell mode (rbash)
    pub fn new(restricted: bool) -> Self {
        Self { restricted, is_env_loaded: Default::default() }
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
            .unwrap_or_else(|| panic!("No API key found. Please set one of: {env_variables}"))
    }

    /// Resolves retry configuration from environment variables or returns
    /// defaults
    fn resolve_retry_config(&self) -> RetryConfig {
        // Parse initial backoff in milliseconds
        let initial_backoff_ms = std::env::var("FORGE_RETRY_INITIAL_BACKOFF_MS")
            .ok()
            .and_then(|val| val.parse::<u64>().ok())
            .unwrap_or(200); // Default value

        // Parse backoff factor
        let backoff_factor = std::env::var("FORGE_RETRY_BACKOFF_FACTOR")
            .ok()
            .and_then(|val| val.parse::<u64>().ok())
            .unwrap_or(2); // Default value

        // Parse maximum retry attempts
        let max_retry_attempts = std::env::var("FORGE_RETRY_MAX_ATTEMPTS")
            .ok()
            .and_then(|val| val.parse::<usize>().ok())
            .unwrap_or(8); // Default value

        // Parse retry status codes
        let retry_status_codes = std::env::var("FORGE_RETRY_STATUS_CODES")
            .ok()
            .map(|val| {
                val.split(',')
                    .filter_map(|code| code.trim().parse::<u16>().ok())
                    .collect::<Vec<u16>>()
            })
            .unwrap_or_else(|| vec![429, 500, 502, 503, 504]); // Default values

        RetryConfig {
            initial_backoff_ms,
            backoff_factor,
            max_retry_attempts,
            retry_status_codes,
        }
    }

    fn get(&self) -> Environment {
        let cwd = std::env::current_dir().unwrap_or(PathBuf::from("."));
        if !self.is_env_loaded.read().map(|v| *v).unwrap_or_default() {
            *self.is_env_loaded.write().unwrap() = true;
            Self::dot_env(&cwd);
        }

        let provider = self.resolve_provider();
        let retry_config = self.resolve_retry_config();

        Environment {
            os: std::env::consts::OS.to_string(),
            pid: std::process::id(),
            cwd,
            shell: self.get_shell_path(),
            base_path: dirs::home_dir()
                .map(|a| a.join("forge"))
                .unwrap_or(PathBuf::from(".").join("forge")),
            home: dirs::home_dir(),
            provider,
            retry_config,
        }
    }

    /// Load all `.env` files with priority to lower (closer) files.
    fn dot_env(cwd: &Path) -> Option<()> {
        let mut paths = vec![];
        let mut current = PathBuf::new();

        for component in cwd.components() {
            current.push(component);
            paths.push(current.clone());
        }

        paths.reverse();

        for path in paths {
            let env_file = path.join(".env");
            if env_file.is_file() {
                dotenv::from_path(&env_file).ok();
            }
        }

        Some(())
    }
}

impl EnvironmentService for ForgeEnvironmentService {
    fn get_environment(&self) -> Environment {
        self.get()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::{env, fs};

    use tempfile::{tempdir, TempDir};

    use super::*;

    fn setup_envs(structure: Vec<(&str, &str)>) -> (TempDir, PathBuf) {
        let root = tempdir().unwrap();
        let root_path = root.path().to_path_buf();

        for (rel_path, content) in &structure {
            let dir = root_path.join(rel_path);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(".env"), content).unwrap();
        }

        let deepest_path = root_path.join(structure[0].0);
        // We MUST return root path, because dropping it will remove temp dir
        (root, deepest_path)
    }

    #[test]
    fn test_load_all_single_env() {
        let (_root, cwd) = setup_envs(vec![("", "TEST_KEY1=VALUE1")]);

        ForgeEnvironmentService::dot_env(&cwd);

        assert_eq!(env::var("TEST_KEY1").unwrap(), "VALUE1");
    }

    #[test]
    fn test_load_all_nested_envs_override() {
        let (_root, cwd) = setup_envs(vec![("a/b", "TEST_KEY2=SUB"), ("a", "TEST_KEY2=ROOT")]);

        ForgeEnvironmentService::dot_env(&cwd);

        assert_eq!(env::var("TEST_KEY2").unwrap(), "SUB");
    }

    #[test]
    fn test_load_all_multiple_keys() {
        let (_root, cwd) = setup_envs(vec![
            ("a/b", "SUB_KEY3=SUB_VAL"),
            ("a", "ROOT_KEY3=ROOT_VAL"),
        ]);

        ForgeEnvironmentService::dot_env(&cwd);

        assert_eq!(env::var("ROOT_KEY3").unwrap(), "ROOT_VAL");
        assert_eq!(env::var("SUB_KEY3").unwrap(), "SUB_VAL");
    }

    #[test]
    fn test_env_precedence_std_env_wins() {
        let (_root, cwd) = setup_envs(vec![
            ("a/b", "TEST_KEY4=SUB_VAL"),
            ("a", "TEST_KEY4=ROOT_VAL"),
        ]);

        env::set_var("TEST_KEY4", "STD_ENV_VAL");

        ForgeEnvironmentService::dot_env(&cwd);

        assert_eq!(env::var("TEST_KEY4").unwrap(), "STD_ENV_VAL");
    }

    #[test]
    fn test_custom_scenario() {
        let (_root, cwd) = setup_envs(vec![("a/b", "A1=1\nB1=2"), ("a", "A1=2\nC1=3")]);

        ForgeEnvironmentService::dot_env(&cwd);

        assert_eq!(env::var("A1").unwrap(), "1");
        assert_eq!(env::var("B1").unwrap(), "2");
        assert_eq!(env::var("C1").unwrap(), "3");
    }

    #[test]
    fn test_custom_scenario_with_std_env_precedence() {
        let (_root, cwd) = setup_envs(vec![("a/b", "A2=1"), ("a", "A2=2")]);

        env::set_var("A2", "STD_ENV");

        ForgeEnvironmentService::dot_env(&cwd);

        assert_eq!(env::var("A2").unwrap(), "STD_ENV");
    }
}
