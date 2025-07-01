use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::collections::HashMap;

use forge_domain::{Environment, Provider, RetryConfig};
use forge_services::EnvironmentInfra;

pub struct ForgeEnvironmentInfra {
    restricted: bool,
    is_env_loaded: RwLock<bool>,
    config_path: Option<PathBuf>,
}

type ProviderSearch = (&'static str, Box<dyn FnOnce(&str) -> Provider>);

impl ForgeEnvironmentInfra {
    /// Creates a new EnvironmentFactory with current working directory
    ///
    /// # Arguments
    /// * `unrestricted` - If true, use unrestricted shell mode (sh/bash) If
    ///   false, use restricted shell mode (rbash)
    pub fn new(restricted: bool, config_path: &Option<PathBuf>) -> Self {
        Self { restricted, is_env_loaded: Default::default(), config_path: config_path.clone() }
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
        let yaml_config = self.load_yaml_config();
        
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
                self.get_config_value(&yaml_config, key).map(|api_key| {
                    let mut provider = fun(&api_key);

                    if let Some(url) = self.get_config_value(&yaml_config, "OPENAI_URL") {
                        provider.open_ai_url(url);
                    }

                    // Check for Anthropic URL override
                    if let Some(url) = self.get_config_value(&yaml_config, "ANTHROPIC_URL") {
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
        let yaml_config = self.load_yaml_config();
        let mut config = RetryConfig::default();

        // Override with YAML config or environment variables
        if let Some(val) = self.get_config_value(&yaml_config, "FORGE_RETRY_INITIAL_BACKOFF_MS") {
            if let Ok(parsed) = val.parse::<u64>() {
                config.initial_backoff_ms = parsed;
            }
        }

        if let Some(val) = self.get_config_value(&yaml_config, "FORGE_RETRY_BACKOFF_FACTOR") {
            if let Ok(parsed) = val.parse::<u64>() {
                config.backoff_factor = parsed;
            }
        }

        if let Some(val) = self.get_config_value(&yaml_config, "FORGE_RETRY_MAX_ATTEMPTS") {
            if let Ok(parsed) = val.parse::<usize>() {
                config.max_retry_attempts = parsed;
            }
        }

        if let Some(val) = self.get_config_value(&yaml_config, "FORGE_RETRY_STATUS_CODES") {
            let status_codes: Vec<u16> = val
                .split(',')
                .filter_map(|code| code.trim().parse::<u16>().ok())
                .collect();
            if !status_codes.is_empty() {
                config.retry_status_codes = status_codes;
            }
        }

        config
    }

    fn resolve_timeout_config(&self) -> forge_domain::HttpConfig {
        let yaml_config = self.load_yaml_config();
        let mut config = forge_domain::HttpConfig::default();
        
        if let Some(val) = self.get_config_value(&yaml_config, "FORGE_HTTP_READ_TIMEOUT") {
            if let Ok(parsed) = val.parse::<u64>() {
                config.read_timeout = parsed;
            }
        }
        if let Some(val) = self.get_config_value(&yaml_config, "FORGE_HTTP_POOL_IDLE_TIMEOUT") {
            if let Ok(parsed) = val.parse::<u64>() {
                config.pool_idle_timeout = parsed;
            }
        }
        if let Some(val) = self.get_config_value(&yaml_config, "FORGE_HTTP_POOL_MAX_IDLE_PER_HOST") {
            if let Ok(parsed) = val.parse::<usize>() {
                config.pool_max_idle_per_host = parsed;
            }
        }
        if let Some(val) = self.get_config_value(&yaml_config, "FORGE_HTTP_MAX_REDIRECTS") {
            if let Ok(parsed) = val.parse::<usize>() {
                config.max_redirects = parsed;
            }
        }

        config
    }

    /// Load YAML configuration file and flatten nested keys using __ separator
    fn load_yaml_config(&self) -> HashMap<String, String> {
        let mut config = HashMap::new();
        
        if let Some(config_path) = &self.config_path {
            if config_path.exists() && config_path.is_file() {
                match std::fs::read_to_string(config_path) {
                    Ok(content) => {
                        match serde_yml::from_str::<serde_yml::Value>(&content) {
                            Ok(yaml_value) => {
                                Self::flatten_yaml(&yaml_value, String::new(), &mut config);
                            }
                            Err(e) => {
                                eprintln!("Warning: Failed to parse YAML config file {}: {}", config_path.display(), e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to read config file {}: {}", config_path.display(), e);
                    }
                }
            }
        }
        
        config
    }

    /// Recursively flatten YAML structure with __ as separator
    fn flatten_yaml(value: &serde_yml::Value, prefix: String, result: &mut HashMap<String, String>) {
        match value {
            serde_yml::Value::Mapping(map) => {
                for (key, val) in map {
                    if let Some(key_str) = key.as_str() {
                        let new_key = if prefix.is_empty() {
                            key_str.to_string()
                        } else {
                            format!("{}__{}", prefix, key_str)
                        };
                        Self::flatten_yaml(val, new_key, result);
                    }
                }
            }
            _ => {
                if let Some(val_str) = Self::yaml_value_to_string(value) {
                    result.insert(prefix, val_str);
                }
            }
        }
    }

    /// Convert YAML value to string representation
    fn yaml_value_to_string(value: &serde_yml::Value) -> Option<String> {
        match value {
            serde_yml::Value::String(s) => Some(s.clone()),
            serde_yml::Value::Number(n) => Some(n.to_string()),
            serde_yml::Value::Bool(b) => Some(b.to_string()),
            serde_yml::Value::Null => None,
            _ => None,
        }
    }

    /// Get configuration value with preference: YAML config > environment variable
    fn get_config_value(&self, yaml_config: &HashMap<String, String>, key: &str) -> Option<String> {
        if let Some(value) = yaml_config.get(key) {
            return Some(value.clone());
        }
        
        std::env::var(key).ok()
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
            max_search_lines: 200,
            fetch_truncation_limit: 40_000,
            max_read_size: 500,
            stdout_max_prefix_length: 200,
            stdout_max_suffix_length: 200,
            http: self.resolve_timeout_config(),
            max_file_size: 256 << 10, // 256 KiB
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

impl EnvironmentInfra for ForgeEnvironmentInfra {
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

        ForgeEnvironmentInfra::dot_env(&cwd);

        assert_eq!(env::var("TEST_KEY1").unwrap(), "VALUE1");
    }

    #[test]
    fn test_load_all_nested_envs_override() {
        let (_root, cwd) = setup_envs(vec![("a/b", "TEST_KEY2=SUB"), ("a", "TEST_KEY2=ROOT")]);

        ForgeEnvironmentInfra::dot_env(&cwd);

        assert_eq!(env::var("TEST_KEY2").unwrap(), "SUB");
    }

    #[test]
    fn test_load_all_multiple_keys() {
        let (_root, cwd) = setup_envs(vec![
            ("a/b", "SUB_KEY3=SUB_VAL"),
            ("a", "ROOT_KEY3=ROOT_VAL"),
        ]);

        ForgeEnvironmentInfra::dot_env(&cwd);

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

        ForgeEnvironmentInfra::dot_env(&cwd);

        assert_eq!(env::var("TEST_KEY4").unwrap(), "STD_ENV_VAL");
    }

    #[test]
    fn test_custom_scenario() {
        let (_root, cwd) = setup_envs(vec![("a/b", "A1=1\nB1=2"), ("a", "A1=2\nC1=3")]);

        ForgeEnvironmentInfra::dot_env(&cwd);

        assert_eq!(env::var("A1").unwrap(), "1");
        assert_eq!(env::var("B1").unwrap(), "2");
        assert_eq!(env::var("C1").unwrap(), "3");
    }

    #[test]
    fn test_custom_scenario_with_std_env_precedence() {
        let (_root, cwd) = setup_envs(vec![("a/b", "A2=1"), ("a", "A2=2")]);

        env::set_var("A2", "STD_ENV");

        ForgeEnvironmentInfra::dot_env(&cwd);

        assert_eq!(env::var("A2").unwrap(), "STD_ENV");
    }

    #[test]
    fn test_retry_config_comprehensive() {
        // Test 1: Default consistency
        {
            // Clean up any existing environment variables first
            env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
            env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
            env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
            env::remove_var("FORGE_RETRY_STATUS_CODES");

            // Verify that the environment service uses the same default as RetryConfig
            let env_service = ForgeEnvironmentInfra::new(false, &None);
            let retry_config_from_env = env_service.resolve_retry_config();
            let default_retry_config = RetryConfig::default();

            assert_eq!(
                retry_config_from_env.max_retry_attempts,
                default_retry_config.max_retry_attempts,
                "Environment service and RetryConfig should have consistent default max_retry_attempts"
            );

            assert_eq!(
                retry_config_from_env.initial_backoff_ms,
                default_retry_config.initial_backoff_ms,
                "Environment service and RetryConfig should have consistent default initial_backoff_ms"
            );

            assert_eq!(
                retry_config_from_env.backoff_factor, default_retry_config.backoff_factor,
                "Environment service and RetryConfig should have consistent default backoff_factor"
            );

            assert_eq!(
                retry_config_from_env.retry_status_codes,
                default_retry_config.retry_status_codes,
                "Environment service and RetryConfig should have consistent default retry_status_codes"
            );
        }

        // Test 2: Environment variable override
        {
            // Clean up any existing environment variables first
            env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
            env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
            env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
            env::remove_var("FORGE_RETRY_STATUS_CODES");

            // Set environment variables to override defaults
            env::set_var("FORGE_RETRY_INITIAL_BACKOFF_MS", "500");
            env::set_var("FORGE_RETRY_BACKOFF_FACTOR", "3");
            env::set_var("FORGE_RETRY_MAX_ATTEMPTS", "5");
            env::set_var("FORGE_RETRY_STATUS_CODES", "429,500,502");

            let env_service = ForgeEnvironmentInfra::new(false, &None);
            let config = env_service.resolve_retry_config();

            assert_eq!(config.initial_backoff_ms, 500);
            assert_eq!(config.backoff_factor, 3);
            assert_eq!(config.max_retry_attempts, 5);
            assert_eq!(config.retry_status_codes, vec![429, 500, 502]);

            // Clean up environment variables
            env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
            env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
            env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
            env::remove_var("FORGE_RETRY_STATUS_CODES");
        }

        // Test 3: Partial environment variable override
        {
            // Clean up any existing environment variables first
            env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
            env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
            env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
            env::remove_var("FORGE_RETRY_STATUS_CODES");

            // Set only some environment variables
            env::set_var("FORGE_RETRY_MAX_ATTEMPTS", "10");
            env::set_var("FORGE_RETRY_STATUS_CODES", "503,504");

            let env_service = ForgeEnvironmentInfra::new(false, &None);
            let config = env_service.resolve_retry_config();
            let default_config = RetryConfig::default();

            // Overridden values
            assert_eq!(config.max_retry_attempts, 10);
            assert_eq!(config.retry_status_codes, vec![503, 504]);

            // Default values should remain
            assert_eq!(config.initial_backoff_ms, default_config.initial_backoff_ms);
            assert_eq!(config.backoff_factor, default_config.backoff_factor);

            // Clean up environment variables
            env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
            env::remove_var("FORGE_RETRY_STATUS_CODES");
        }

        // Test 4: Invalid environment variable values
        {
            // Clean up any existing environment variables first
            env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
            env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
            env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
            env::remove_var("FORGE_RETRY_STATUS_CODES");

            // Set invalid environment variables
            env::set_var("FORGE_RETRY_INITIAL_BACKOFF_MS", "invalid");
            env::set_var("FORGE_RETRY_BACKOFF_FACTOR", "not_a_number");
            env::set_var("FORGE_RETRY_MAX_ATTEMPTS", "abc");
            env::set_var("FORGE_RETRY_STATUS_CODES", "invalid,codes,here");

            let env_service = ForgeEnvironmentInfra::new(false, &None);
            let config = env_service.resolve_retry_config();
            let default_config = RetryConfig::default();

            // Should fall back to defaults when parsing fails
            assert_eq!(config.initial_backoff_ms, default_config.initial_backoff_ms);
            assert_eq!(config.backoff_factor, default_config.backoff_factor);
            assert_eq!(config.max_retry_attempts, default_config.max_retry_attempts);
            assert_eq!(config.retry_status_codes, default_config.retry_status_codes);

            // Clean up environment variables
            env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
            env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
            env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
            env::remove_var("FORGE_RETRY_STATUS_CODES");
        }
    }

    #[test]
    fn test_yaml_config_loading() {
        use tempfile::NamedTempFile;
        use std::io::Write;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "FORGE_KEY: \"test_key_from_yaml\"\nOPENAI_URL: \"https://api.custom.openai.com\"\nretry:\n  config:\n    max_attempts: 5\n    initial_backoff_ms: 1000\nnested:\n  deep:\n    value: \"deep_value\"").unwrap();

        let config_path = Some(temp_file.path().to_path_buf());
        let env_service = ForgeEnvironmentInfra::new(false, &config_path);
        let yaml_config = env_service.load_yaml_config();

        // Test direct keys
        assert_eq!(yaml_config.get("FORGE_KEY"), Some(&"test_key_from_yaml".to_string()));
        assert_eq!(yaml_config.get("OPENAI_URL"), Some(&"https://api.custom.openai.com".to_string()));

        // Test nested keys with __ separator
        assert_eq!(yaml_config.get("retry__config__max_attempts"), Some(&"5".to_string()));
        assert_eq!(yaml_config.get("retry__config__initial_backoff_ms"), Some(&"1000".to_string()));
        assert_eq!(yaml_config.get("nested__deep__value"), Some(&"deep_value".to_string()));
    }

    #[test]
    fn test_yaml_config_precedence_over_env() {
        use tempfile::NamedTempFile;
        use std::io::Write;

        // Set environment variable
        env::set_var("FORGE_RETRY_MAX_ATTEMPTS", "3");

        // Create YAML config that should override the env var
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "FORGE_RETRY_MAX_ATTEMPTS: 7").unwrap();

        let config_path = Some(temp_file.path().to_path_buf());
        let env_service = ForgeEnvironmentInfra::new(false, &config_path);
        let yaml_config = env_service.load_yaml_config();

        // YAML should take precedence
        assert_eq!(env_service.get_config_value(&yaml_config, "FORGE_RETRY_MAX_ATTEMPTS"), 
                   Some("7".to_string()));

        // Clean up
        env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
    }
}
