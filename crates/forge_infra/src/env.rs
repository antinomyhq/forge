use std::path::{Path, PathBuf};

use forge_domain::{Environment, Provider, RetryConfig};

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
            .unwrap_or(3); // Default value

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
        Self::load_all(&cwd);
        
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
    fn load_all(cwd: &Path) -> Option<()> {
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

impl forge_domain::EnvironmentService for ForgeEnvironmentService {
    fn get_environment(&self) -> Environment {
        self.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::env;
    use tempfile::tempdir;

    fn write_env_file(dir: &Path, content: &str) {
        let env_path = dir.join(".env");
        fs::write(&env_path, content).unwrap();
    }

    #[test]
    fn test_load_all_single_env() {
        let dir = tempdir().unwrap();
        write_env_file(dir.path(), "TEST_KEY1=VALUE1");

        ForgeEnvironmentService::load_all(dir.path());

        assert_eq!(env::var("TEST_KEY1").unwrap(), "VALUE1");
    }

    #[test]
    fn test_load_all_nested_envs_override() {
        let root = tempdir().unwrap();
        let subdir = root.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        write_env_file(root.path(), "TEST_KEY2=ROOT");
        write_env_file(&subdir, "TEST_KEY2=SUB");

        ForgeEnvironmentService::load_all(&subdir);

        assert_eq!(env::var("TEST_KEY2").unwrap(), "SUB");
    }

    #[test]
    fn test_load_all_multiple_keys() {
        let root = tempdir().unwrap();
        let subdir = root.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        write_env_file(root.path(), "ROOT_KEY3=ROOT_VAL");
        write_env_file(&subdir, "SUB_KEY3=SUB_VAL");

        ForgeEnvironmentService::load_all(&subdir);

        assert_eq!(env::var("ROOT_KEY3").unwrap(), "ROOT_VAL");
        assert_eq!(env::var("SUB_KEY3").unwrap(), "SUB_VAL");
    }

    #[test]
    fn test_env_precedence_std_env_wins() {
        let root = tempdir().unwrap();
        let subdir = root.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        write_env_file(root.path(), "TEST_KEY4=ROOT_VAL");
        write_env_file(&subdir, "TEST_KEY4=SUB_VAL");

        env::set_var("TEST_KEY4", "STD_ENV_VAL");

        ForgeEnvironmentService::load_all(&subdir);

        assert_eq!(env::var("TEST_KEY4").unwrap(), "STD_ENV_VAL");
    }
}

