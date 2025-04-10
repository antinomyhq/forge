use std::path::PathBuf;
use std::sync::Mutex;

use forge_domain::{Environment, Provider, RetryConfig};

pub struct ForgeEnvironmentService {
    restricted: bool,
    // Track the current working directory with interior mutability
    current_cwd: Mutex<Option<PathBuf>>,
}

type ProviderSearch = (&'static str, Box<dyn FnOnce(&str) -> Provider>);

impl ForgeEnvironmentService {
    /// Creates a new EnvironmentFactory with current working directory
    ///
    /// # Arguments
    /// * `unrestricted` - If true, use unrestricted shell mode (sh/bash) If
    ///   false, use restricted shell mode (rbash)
    pub fn new(restricted: bool) -> Self {
        Self { restricted, current_cwd: Mutex::new(None) }
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
    /// Returns a default provider if no API key is found in the environment
    /// instead of panicking
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
            .unwrap_or_else(|| {
                // Instead of panicking, return a default provider
                // In a desktop app context, we should show a UI for entering keys
                // FIXME: Handle this more gracefully in the future
                eprintln!(
                    "No API key found in environment variables. Expected one of: {}",
                    env_variables
                );
                eprintln!("Using default OpenAI provider. Please set API keys in environment.");

                // Return a default provider with empty key - this will fail on API calls
                // but won't crash the application startup
                Provider::openai("")
            })
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
        // Environment loading is now done at application startup
        // to avoid needing permissions to access the filesystem for dotenv

        // Use the custom cwd if set, otherwise use the current directory
        let cwd = {
            let custom_cwd = self.current_cwd.lock().unwrap();
            match &*custom_cwd {
                Some(path) => path.clone(),
                None => std::env::current_dir().unwrap_or(PathBuf::from(".")),
            }
        };

        let provider = self.resolve_provider();
        let retry_config = self.resolve_retry_config();

        Environment {
            os: std::env::consts::OS.to_string(),
            pid: std::process::id(),
            cwd,
            shell: self.get_shell_path(),
            base_path: dirs::config_dir()
                .map(|a| a.join("forge"))
                .unwrap_or(PathBuf::from(".").join(".forge")),
            home: dirs::home_dir(),
            provider,
            retry_config,
        }
    }
}

impl forge_domain::EnvironmentService for ForgeEnvironmentService {
    fn set_cwd(&self, cwd: PathBuf) -> anyhow::Result<()> {
        // Validate that the directory exists and is accessible
        if !cwd.is_dir() {
            return Err(anyhow::anyhow!(
                "Path is not a directory: {}",
                cwd.display()
            ));
        }

        // Update the custom cwd
        let mut custom_cwd = self.current_cwd.lock().unwrap();
        *custom_cwd = Some(cwd);

        Ok(())
    }

    fn get_environment(&self) -> Environment {
        self.get()
    }
}
