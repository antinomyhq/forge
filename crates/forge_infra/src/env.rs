use std::path::{Path, PathBuf};

use forge_domain::{Environment, Provider, RetryConfig};
use forge_services::EnvironmentInfra;
use reqwest::Url;

#[derive(Clone)]
pub struct ForgeEnvironmentInfra {
    restricted: bool,
    cwd: PathBuf,
}

impl ForgeEnvironmentInfra {
    /// Creates a new EnvironmentFactory with specified working directory
    ///
    /// # Arguments
    /// * `restricted` - If true, use restricted shell mode (rbash) If false,
    ///   use unrestricted shell mode (sh/bash)
    /// * `cwd` - Required working directory path
    pub fn new(restricted: bool, cwd: PathBuf) -> Self {
        Self::dot_env(&cwd);
        Self { restricted, cwd }
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

    /// Resolves retry configuration from environment variables or returns
    /// defaults
    fn resolve_retry_config(&self) -> RetryConfig {
        let mut config = RetryConfig::default();

        // Override with environment variables if available
        if let Ok(val) = std::env::var("FORGE_RETRY_INITIAL_BACKOFF_MS")
            && let Ok(parsed) = val.parse::<u64>()
        {
            config.initial_backoff_ms = parsed;
        }

        if let Ok(val) = std::env::var("FORGE_RETRY_BACKOFF_FACTOR")
            && let Ok(parsed) = val.parse::<u64>()
        {
            config.backoff_factor = parsed;
        }

        if let Ok(val) = std::env::var("FORGE_RETRY_MAX_ATTEMPTS")
            && let Ok(parsed) = val.parse::<usize>()
        {
            config.max_retry_attempts = parsed;
        }

        if let Ok(val) = std::env::var("FORGE_RETRY_STATUS_CODES") {
            let status_codes: Vec<u16> = val
                .split(',')
                .filter_map(|code| code.trim().parse::<u16>().ok())
                .collect();
            if !status_codes.is_empty() {
                config.retry_status_codes = status_codes;
            }
        }

        if let Some(parsed) = std::env::var("FORGE_SUPPRESS_RETRY_ERRORS")
            .ok()
            .and_then(|val| val.parse::<bool>().ok())
        {
            config.suppress_retry_errors = parsed;
        }

        config
    }

    fn resolve_timeout_config(&self) -> forge_domain::HttpConfig {
        let mut config = forge_domain::HttpConfig::default();
        if let Ok(val) = std::env::var("FORGE_HTTP_CONNECT_TIMEOUT")
            && let Ok(parsed) = val.parse::<u64>()
        {
            config.connect_timeout = parsed;
        }
        if let Ok(val) = std::env::var("FORGE_HTTP_READ_TIMEOUT")
            && let Ok(parsed) = val.parse::<u64>()
        {
            config.read_timeout = parsed;
        }
        if let Ok(val) = std::env::var("FORGE_HTTP_POOL_IDLE_TIMEOUT")
            && let Ok(parsed) = val.parse::<u64>()
        {
            config.pool_idle_timeout = parsed;
        }
        if let Ok(val) = std::env::var("FORGE_HTTP_POOL_MAX_IDLE_PER_HOST")
            && let Ok(parsed) = val.parse::<usize>()
        {
            config.pool_max_idle_per_host = parsed;
        }
        if let Ok(val) = std::env::var("FORGE_HTTP_MAX_REDIRECTS")
            && let Ok(parsed) = val.parse::<usize>()
        {
            config.max_redirects = parsed;
        }

        config
    }

    fn get(&self) -> Environment {
        let cwd = self.cwd.clone();
        let retry_config = self.resolve_retry_config();

        let forge_api_url = self
            .get_env_var("FORGE_API_URL")
            .as_ref()
            .and_then(|url| Url::parse(url.as_str()).ok())
            .unwrap_or_else(|| Url::parse(Provider::FORGE_URL).unwrap());

        Environment {
            os: std::env::consts::OS.to_string(),
            pid: std::process::id(),
            cwd,
            shell: self.get_shell_path(),
            base_path: dirs::home_dir()
                .map(|a| a.join("forge"))
                .unwrap_or(PathBuf::from(".").join("forge")),
            home: dirs::home_dir(),
            retry_config,
            max_search_lines: 200,
            fetch_truncation_limit: 40_000,
            max_read_size: 500,
            stdout_max_prefix_length: 200,
            stdout_max_suffix_length: 200,
            http: self.resolve_timeout_config(),
            max_file_size: 256 << 10, // 256 KiB
            forge_api_url,
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

    fn get_env_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::{env, fs};

    use tempfile::{TempDir, tempdir};

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

        unsafe {
            env::set_var("TEST_KEY4", "STD_ENV_VAL");
        }

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

        unsafe {
            env::set_var("A2", "STD_ENV");
        }

        ForgeEnvironmentInfra::dot_env(&cwd);

        assert_eq!(env::var("A2").unwrap(), "STD_ENV");
    }

    #[test]
    fn test_retry_config_comprehensive() {
        // Test 1: Default consistency
        {
            // Clean up any existing environment variables first
            unsafe {
                env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
                env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
                env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
                env::remove_var("FORGE_RETRY_STATUS_CODES");
                env::remove_var("FORGE_SUPPRESS_RETRY_ERRORS");
            }

            // Verify that the environment service uses the same default as RetryConfig
            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let retry_config_from_env = env_service.resolve_retry_config();
            let default_retry_config = RetryConfig::default();

            assert_eq!(
                retry_config_from_env.max_retry_attempts, default_retry_config.max_retry_attempts,
                "Environment service and RetryConfig should have consistent default max_retry_attempts"
            );

            assert_eq!(
                retry_config_from_env.initial_backoff_ms, default_retry_config.initial_backoff_ms,
                "Environment service and RetryConfig should have consistent default initial_backoff_ms"
            );

            assert_eq!(
                retry_config_from_env.backoff_factor, default_retry_config.backoff_factor,
                "Environment service and RetryConfig should have consistent default backoff_factor"
            );

            assert_eq!(
                retry_config_from_env.retry_status_codes, default_retry_config.retry_status_codes,
                "Environment service and RetryConfig should have consistent default retry_status_codes"
            );

            assert_eq!(
                retry_config_from_env.suppress_retry_errors,
                default_retry_config.suppress_retry_errors,
                "Environment service and RetryConfig should have consistent default suppress_retry_errors"
            );
        }

        // Test 2: Environment variable override
        {
            // Clean up any existing environment variables first
            unsafe {
                env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
                env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
                env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
                env::remove_var("FORGE_RETRY_STATUS_CODES");
                env::remove_var("FORGE_SUPPRESS_RETRY_ERRORS");
            }

            // Set environment variables to override defaults
            unsafe {
                env::set_var("FORGE_RETRY_INITIAL_BACKOFF_MS", "500");
                env::set_var("FORGE_RETRY_BACKOFF_FACTOR", "3");
                env::set_var("FORGE_RETRY_MAX_ATTEMPTS", "5");
                env::set_var("FORGE_RETRY_STATUS_CODES", "429,500,502");
                env::set_var("FORGE_SUPPRESS_RETRY_ERRORS", "true");
            }

            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_retry_config();

            assert_eq!(config.initial_backoff_ms, 500);
            assert_eq!(config.backoff_factor, 3);
            assert_eq!(config.max_retry_attempts, 5);
            assert_eq!(config.retry_status_codes, vec![429, 500, 502]);
            assert_eq!(config.suppress_retry_errors, true);

            // Clean up environment variables
            unsafe {
                env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
                env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
                env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
                env::remove_var("FORGE_RETRY_STATUS_CODES");
                env::remove_var("FORGE_SUPPRESS_RETRY_ERRORS");
            }
        }

        // Test 3: Partial environment variable override
        {
            // Clean up any existing environment variables first
            unsafe {
                env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
                env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
                env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
                env::remove_var("FORGE_RETRY_STATUS_CODES");
                env::remove_var("FORGE_SUPPRESS_RETRY_ERRORS");
            }

            // Set only some environment variables
            unsafe {
                env::set_var("FORGE_RETRY_MAX_ATTEMPTS", "10");
                env::set_var("FORGE_RETRY_STATUS_CODES", "503,504");
            }

            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_retry_config();
            let default_config = RetryConfig::default();

            // Overridden values
            assert_eq!(config.max_retry_attempts, 10);
            assert_eq!(config.retry_status_codes, vec![503, 504]);

            // Default values should remain
            assert_eq!(config.initial_backoff_ms, default_config.initial_backoff_ms);
            assert_eq!(config.backoff_factor, default_config.backoff_factor);

            // Clean up environment variables
            unsafe {
                env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
                env::remove_var("FORGE_RETRY_STATUS_CODES");
            }
        }

        // Test 4: Invalid environment variable values
        {
            // Clean up any existing environment variables first
            unsafe {
                env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
                env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
                env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
                env::remove_var("FORGE_RETRY_STATUS_CODES");
                env::remove_var("FORGE_SUPPRESS_RETRY_ERRORS");
            }

            // Set invalid environment variables
            unsafe {
                env::set_var("FORGE_RETRY_INITIAL_BACKOFF_MS", "invalid");
                env::set_var("FORGE_RETRY_BACKOFF_FACTOR", "not_a_number");
                env::set_var("FORGE_RETRY_MAX_ATTEMPTS", "abc");
                env::set_var("FORGE_RETRY_STATUS_CODES", "invalid,codes,here");
            }

            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_retry_config();
            let default_config = RetryConfig::default();

            // Should fall back to defaults when parsing fails
            assert_eq!(config.initial_backoff_ms, default_config.initial_backoff_ms);
            assert_eq!(config.backoff_factor, default_config.backoff_factor);
            assert_eq!(config.max_retry_attempts, default_config.max_retry_attempts);
            assert_eq!(config.retry_status_codes, default_config.retry_status_codes);

            // Clean up environment variables
            unsafe {
                env::remove_var("FORGE_RETRY_INITIAL_BACKOFF_MS");
                env::remove_var("FORGE_RETRY_BACKOFF_FACTOR");
                env::remove_var("FORGE_RETRY_MAX_ATTEMPTS");
                env::remove_var("FORGE_RETRY_STATUS_CODES");
                env::remove_var("FORGE_SUPPRESS_RETRY_ERRORS");
            }
        }

        // Test 5: FORGE_SUPPRESS_RETRY_ERRORS environment variable
        {
            // Clean up any existing environment variables first
            unsafe {
                env::remove_var("FORGE_SUPPRESS_RETRY_ERRORS");
            }

            // Test default value (false)
            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_retry_config();
            assert_eq!(config.suppress_retry_errors, false);

            // Test setting to true
            unsafe {
                env::set_var("FORGE_SUPPRESS_RETRY_ERRORS", "true");
            }

            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_retry_config();
            assert_eq!(config.suppress_retry_errors, true);

            // Test setting to false explicitly
            unsafe {
                env::set_var("FORGE_SUPPRESS_RETRY_ERRORS", "false");
            }

            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_retry_config();
            assert_eq!(config.suppress_retry_errors, false);

            // Test invalid value (should use default)
            unsafe {
                env::set_var("FORGE_SUPPRESS_RETRY_ERRORS", "invalid");
            }

            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_retry_config();
            assert_eq!(config.suppress_retry_errors, false); // Should fallback to default

            // Clean up environment variable
            unsafe {
                env::remove_var("FORGE_SUPPRESS_RETRY_ERRORS");
            }
        }
    }

    #[test]
    fn test_http_config_environment_variables() {
        // Clean up any existing environment variables first
        unsafe {
            env::remove_var("FORGE_HTTP_CONNECT_TIMEOUT");
            env::remove_var("FORGE_HTTP_READ_TIMEOUT");
            env::remove_var("FORGE_HTTP_POOL_IDLE_TIMEOUT");
            env::remove_var("FORGE_HTTP_POOL_MAX_IDLE_PER_HOST");
            env::remove_var("FORGE_HTTP_MAX_REDIRECTS");
        }

        // Test default values
        {
            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_timeout_config();
            let default_config = forge_domain::HttpConfig::default();

            assert_eq!(config.connect_timeout, default_config.connect_timeout);
            assert_eq!(config.read_timeout, default_config.read_timeout);
            assert_eq!(config.pool_idle_timeout, default_config.pool_idle_timeout);
            assert_eq!(
                config.pool_max_idle_per_host,
                default_config.pool_max_idle_per_host
            );
            assert_eq!(config.max_redirects, default_config.max_redirects);
        }

        // Test environment variable overrides
        {
            unsafe {
                env::set_var("FORGE_HTTP_CONNECT_TIMEOUT", "30");
                env::set_var("FORGE_HTTP_READ_TIMEOUT", "120");
                env::set_var("FORGE_HTTP_POOL_IDLE_TIMEOUT", "180");
                env::set_var("FORGE_HTTP_POOL_MAX_IDLE_PER_HOST", "10");
                env::set_var("FORGE_HTTP_MAX_REDIRECTS", "20");
            }

            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_timeout_config();

            assert_eq!(config.connect_timeout, 30);
            assert_eq!(config.read_timeout, 120);
            assert_eq!(config.pool_idle_timeout, 180);
            assert_eq!(config.pool_max_idle_per_host, 10);
            assert_eq!(config.max_redirects, 20);

            // Clean up environment variables
            unsafe {
                env::remove_var("FORGE_HTTP_CONNECT_TIMEOUT");
                env::remove_var("FORGE_HTTP_READ_TIMEOUT");
                env::remove_var("FORGE_HTTP_POOL_IDLE_TIMEOUT");
                env::remove_var("FORGE_HTTP_POOL_MAX_IDLE_PER_HOST");
                env::remove_var("FORGE_HTTP_MAX_REDIRECTS");
            }
        }

        // Test partial environment variable override (specifically connect_timeout)
        {
            unsafe {
                env::set_var("FORGE_HTTP_CONNECT_TIMEOUT", "15");
            }

            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_timeout_config();
            let default_config = forge_domain::HttpConfig::default();

            // Overridden value
            assert_eq!(config.connect_timeout, 15);

            // Default values should remain
            assert_eq!(config.read_timeout, default_config.read_timeout);
            assert_eq!(config.pool_idle_timeout, default_config.pool_idle_timeout);
            assert_eq!(
                config.pool_max_idle_per_host,
                default_config.pool_max_idle_per_host
            );
            assert_eq!(config.max_redirects, default_config.max_redirects);

            // Clean up environment variables
            unsafe {
                env::remove_var("FORGE_HTTP_CONNECT_TIMEOUT");
            }
        }

        // Test invalid environment variable values
        {
            unsafe {
                env::set_var("FORGE_HTTP_CONNECT_TIMEOUT", "invalid");
            }

            let env_service = ForgeEnvironmentInfra::new(false, PathBuf::from("."));
            let config = env_service.resolve_timeout_config();
            let default_config = forge_domain::HttpConfig::default();

            // Should fall back to default when parsing fails
            assert_eq!(config.connect_timeout, default_config.connect_timeout);

            // Clean up environment variables
            unsafe {
                env::remove_var("FORGE_HTTP_CONNECT_TIMEOUT");
            }
        }
    }
}
