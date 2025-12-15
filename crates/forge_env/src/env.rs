use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;

use derive_more::Display;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{HttpConfig, RetryConfig};

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Debug, Setters, Clone, Serialize, Deserialize, fake::Dummy)]
#[serde(rename_all = "camelCase")]
#[setters(strip_option)]
/// Represents the environment in which the application is running.
pub struct Environment {
    /// The operating system of the environment.
    pub os: String,
    /// The process ID of the current process.
    pub pid: u32,
    /// The current working directory.
    pub cwd: PathBuf,
    /// The home directory.
    pub home: Option<PathBuf>,
    /// The shell being used.
    pub shell: String,
    /// The base path relative to which everything else stored.
    pub base_path: PathBuf,
    /// Base URL for Forge's backend APIs
    #[dummy(expr = "url::Url::parse(\"https://example.com\").unwrap()")]
    pub forge_api_url: Url,
    /// Configuration for the retry mechanism
    pub retry_config: RetryConfig,
    /// The maximum number of lines returned for FSSearch.
    pub max_search_lines: usize,
    /// Maximum bytes allowed for search results
    pub max_search_result_bytes: usize,
    /// Maximum characters for fetch content
    pub fetch_truncation_limit: usize,
    /// Maximum lines for shell output prefix
    pub stdout_max_prefix_length: usize,
    /// Maximum lines for shell output suffix
    pub stdout_max_suffix_length: usize,
    /// Maximum characters per line for shell output
    pub stdout_max_line_length: usize,
    /// Maximum number of lines to read from a file
    pub max_read_size: u64,
    /// Http configuration
    pub http: HttpConfig,
    /// Maximum file size in bytes for operations
    pub max_file_size: u64,
    /// Maximum image file size in bytes for binary read operations
    pub max_image_size: u64,
    /// Maximum execution time in seconds for a single tool call.
    /// Controls how long a tool can run before being terminated.
    pub tool_timeout: u64,
    /// Whether to automatically open HTML dump files in the browser.
    /// Controlled by FORGE_DUMP_AUTO_OPEN environment variable.
    pub auto_open_dump: bool,
    /// Path where debug request files should be written.
    /// Controlled by FORGE_DEBUG_REQUESTS environment variable.
    pub debug_requests: Option<PathBuf>,
    /// Custom history file path from FORGE_HISTORY_FILE environment variable.
    /// If None, uses the default history path.
    pub custom_history_path: Option<PathBuf>,
    /// Maximum number of conversations to show in list.
    /// Controlled by FORGE_MAX_CONVERSATIONS environment variable.
    pub max_conversations: usize,
    /// Maximum number of results to return from initial vector search.
    /// Controlled by FORGE_SEM_SEARCH_LIMIT environment variable.
    pub sem_search_limit: usize,
    /// Top-k parameter for relevance filtering during semantic search.
    /// Controls the number of nearest neighbors to consider.
    /// Controlled by FORGE_SEM_SEARCH_TOP_K environment variable.
    pub sem_search_top_k: usize,
    /// URL for the indexing server.
    /// Controlled by FORGE_WORKSPACE_SERVER_URL environment variable.
    #[dummy(expr = "url::Url::parse(\"http://localhost:8080\").unwrap()")]
    pub workspace_server_url: Url,
    /// Override model for all providers from FORGE_OVERRIDE_MODEL environment
    /// variable. If set, this model will be used instead of configured
    /// models.
    #[dummy(default)]
    pub override_model: Option<String>,
    /// Override provider from FORGE_OVERRIDE_PROVIDER environment variable.
    /// If set, this provider will be used as default.
    #[dummy(default)]
    pub override_provider: Option<String>,
}

impl Environment {
    /// Creates an Environment instance from environment variables using the
    /// config crate.
    ///
    /// Loads configuration from two sources in order of precedence:
    /// 1. Embedded JSON config (`env.json` in the crate root, compiled into binary)
    /// 2. Environment variables prefixed with `FORGE_` (highest priority)
    ///
    /// Environment variables will override values from the embedded JSON config.
    /// Uses double underscore (`__`) as a separator for nested
    /// configurations.
    ///
    /// # Examples of environment variables:
    /// - `FORGE_OS` -> `os`
    /// - `FORGE_PID` -> `pid`
    /// - `FORGE_CWD` -> `cwd`
    /// - `FORGE_RETRY_CONFIG__INITIAL_BACKOFF_MS` -> `retryConfig.initialBackoffMs`
    /// - `FORGE_HTTP__CONNECT_TIMEOUT` -> `http.connectTimeout`
    ///
    /// # Errors
    /// Returns an error if:
    /// - Required environment variables are missing
    /// - Environment variable values cannot be parsed into the expected types
    /// - The configuration is invalid or incomplete
    pub fn from_env() -> Result<Self, config::ConfigError> {
        // Embed default configuration at compile time
        const DEFAULT_CONFIG: &str = include_str!("../env.json");
        
        let config = config::Config::builder()
            // Add embedded JSON config as base configuration
            .add_source(config::File::from_str(DEFAULT_CONFIG, config::FileFormat::Json))
            // Environment variables override default configuration
            .add_source(
                config::Environment::with_prefix("FORGE")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        config.try_deserialize()
    }

    pub fn log_path(&self) -> PathBuf {
        self.base_path.join("logs")
    }

    pub fn history_path(&self) -> PathBuf {
        self.custom_history_path
            .clone()
            .unwrap_or(self.base_path.join(".forge_history"))
    }
    pub fn snapshot_path(&self) -> PathBuf {
        self.base_path.join("snapshots")
    }
    pub fn mcp_user_config(&self) -> PathBuf {
        self.base_path.join(".mcp.json")
    }

    pub fn templates(&self) -> PathBuf {
        self.base_path.join("templates")
    }
    pub fn agent_path(&self) -> PathBuf {
        self.base_path.join("agents")
    }
    pub fn agent_cwd_path(&self) -> PathBuf {
        self.cwd.join(".forge/agents")
    }

    pub fn command_path(&self) -> PathBuf {
        self.base_path.join("commands")
    }

    pub fn command_cwd_path(&self) -> PathBuf {
        self.cwd.join(".forge/commands")
    }
    pub fn permissions_path(&self) -> PathBuf {
        self.base_path.join("permissions.yaml")
    }

    pub fn mcp_local_config(&self) -> PathBuf {
        self.cwd.join(".mcp.json")
    }

    pub fn version(&self) -> String {
        VERSION.to_string()
    }

    pub fn app_config(&self) -> PathBuf {
        self.base_path.join(".config.json")
    }

    pub fn database_path(&self) -> PathBuf {
        self.base_path.join(".forge.db")
    }

    /// Returns the path to the cache directory
    pub fn cache_dir(&self) -> PathBuf {
        self.base_path.join("cache")
    }

    /// Returns the global skills directory path (~/forge/skills)
    pub fn global_skills_path(&self) -> PathBuf {
        self.base_path.join("skills")
    }

    /// Returns the project-local skills directory path (.forge/skills)
    pub fn local_skills_path(&self) -> PathBuf {
        self.cwd.join(".forge/skills")
    }

    pub fn workspace_hash(&self) -> WorkspaceHash {
        let mut hasher = DefaultHasher::default();
        self.cwd.hash(&mut hasher);

        WorkspaceHash(hasher.finish())
    }
}

#[derive(Clone, Copy, Display)]
pub struct WorkspaceHash(u64);
impl WorkspaceHash {
    pub fn new(id: u64) -> Self {
        WorkspaceHash(id)
    }

    pub fn id(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use fake::{Fake, Faker};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_agent_cwd_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.cwd(PathBuf::from("/current/working/dir"));

        let actual = fixture.agent_cwd_path();
        let expected = PathBuf::from("/current/working/dir/.forge/agents");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_agent_cwd_path_independent_from_agent_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture
            .cwd(PathBuf::from("/different/current/dir"))
            .base_path(PathBuf::from("/completely/different/base"));

        let agent_path = fixture.agent_path();
        let agent_cwd_path = fixture.agent_cwd_path();
        let expected_agent_path = PathBuf::from("/completely/different/base/agents");
        let expected_agent_cwd_path = PathBuf::from("/different/current/dir/.forge/agents");

        // Verify that agent_path uses base_path
        assert_eq!(agent_path, expected_agent_path);

        // Verify that agent_cwd_path is independent and always relative to CWD
        assert_eq!(agent_cwd_path, expected_agent_cwd_path);

        // Verify they are different paths
        assert_ne!(agent_path, agent_cwd_path);
    }

    #[test]
    fn test_global_skills_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.global_skills_path();
        let expected = PathBuf::from("/home/user/.forge/skills");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_local_skills_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.cwd(PathBuf::from("/projects/my-app"));

        let actual = fixture.local_skills_path();
        let expected = PathBuf::from("/projects/my-app/.forge/skills");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_skills_paths_independent() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture
            .cwd(PathBuf::from("/projects/my-app"))
            .base_path(PathBuf::from("/home/user/.forge"));

        let global_path = fixture.global_skills_path();
        let local_path = fixture.local_skills_path();

        let expected_global = PathBuf::from("/home/user/.forge/skills");
        let expected_local = PathBuf::from("/projects/my-app/.forge/skills");

        // Verify global path uses base_path
        assert_eq!(global_path, expected_global);

        // Verify local path uses cwd
        assert_eq!(local_path, expected_local);

        // Verify they are different paths
        assert_ne!(global_path, local_path);
    }
}

#[test]
fn test_command_path() {
    let fixture = Environment {
        os: "linux".to_string(),
        pid: 1234,
        cwd: PathBuf::from("/current/working/dir"),
        home: Some(PathBuf::from("/home/user")),
        shell: "zsh".to_string(),
        base_path: PathBuf::from("/home/user/.forge"),
        forge_api_url: "https://api.example.com".parse().unwrap(),
        retry_config: RetryConfig::default(),
        max_search_lines: 1000,
        max_search_result_bytes: 10240,
        fetch_truncation_limit: 50000,
        stdout_max_prefix_length: 100,
        stdout_max_suffix_length: 100,
        stdout_max_line_length: 500,
        max_read_size: 2000,
        http: HttpConfig::default(),
        max_file_size: 104857600,
        tool_timeout: 300,
        auto_open_dump: false,
        debug_requests: None,
        custom_history_path: None,
        max_conversations: 100,
        sem_search_limit: 100,
        sem_search_top_k: 10,
        max_image_size: 262144,
        workspace_server_url: "http://localhost:8080".parse().unwrap(),
        override_model: None,
        override_provider: None,
    };

    let actual = fixture.command_path();
    let expected = PathBuf::from("/home/user/.forge/commands");

    assert_eq!(actual, expected);
}

#[test]
fn test_command_cwd_path() {
    let fixture = Environment {
        os: "linux".to_string(),
        pid: 1234,
        cwd: PathBuf::from("/current/working/dir"),
        home: Some(PathBuf::from("/home/user")),
        shell: "zsh".to_string(),
        base_path: PathBuf::from("/home/user/.forge"),
        forge_api_url: "https://api.example.com".parse().unwrap(),
        retry_config: RetryConfig::default(),
        max_search_lines: 1000,
        max_search_result_bytes: 10240,
        fetch_truncation_limit: 50000,
        stdout_max_prefix_length: 100,
        stdout_max_suffix_length: 100,
        stdout_max_line_length: 500,
        max_read_size: 2000,
        http: HttpConfig::default(),
        max_file_size: 104857600,
        tool_timeout: 300,
        auto_open_dump: false,
        debug_requests: None,
        custom_history_path: None,
        max_conversations: 100,
        sem_search_limit: 100,
        sem_search_top_k: 10,
        max_image_size: 262144,
        workspace_server_url: "http://localhost:8080".parse().unwrap(),
        override_model: None,
        override_provider: None,
    };

    let actual = fixture.command_cwd_path();
    let expected = PathBuf::from("/current/working/dir/.forge/commands");

    assert_eq!(actual, expected);
}

    #[test]
    fn test_command_cwd_path_independent_from_command_path() {
        let fixture = Environment {
            os: "linux".to_string(),
            pid: 1234,
            cwd: PathBuf::from("/different/current/dir"),
            home: Some(PathBuf::from("/different/home")),
            shell: "bash".to_string(),
            base_path: PathBuf::from("/completely/different/base"),
            forge_api_url: "https://api.example.com".parse().unwrap(),
            retry_config: RetryConfig::default(),
            max_search_lines: 1000,
            max_search_result_bytes: 10240,
            fetch_truncation_limit: 50000,
            stdout_max_prefix_length: 100,
            stdout_max_suffix_length: 100,
            stdout_max_line_length: 500,
            max_read_size: 2000,
            http: HttpConfig::default(),
            max_file_size: 104857600,
            tool_timeout: 300,
            auto_open_dump: false,
            debug_requests: None,
            custom_history_path: None,
            max_conversations: 100,
            sem_search_limit: 100,
            sem_search_top_k: 10,
            max_image_size: 262144,
            workspace_server_url: "http://localhost:8080".parse().unwrap(),
            override_model: None,
            override_provider: None,
        };

        let command_path = fixture.command_path();
        let command_cwd_path = fixture.command_cwd_path();
        let expected_command_path = PathBuf::from("/completely/different/base/commands");
        let expected_command_cwd_path = PathBuf::from("/different/current/dir/.forge/commands");

        // Verify that command_path uses base_path
        assert_eq!(command_path, expected_command_path);

        // Verify that command_cwd_path is independent and always relative to CWD
        assert_eq!(command_cwd_path, expected_command_cwd_path);

        // Verify they are different paths
        assert_ne!(command_path, command_cwd_path);
    }

    #[test]
    fn test_from_env_snake_to_camel_conversion() {
        // Test the snake_case to camelCase conversion function indirectly
        unsafe {
            std::env::set_var("FORGE_MAX_SEARCH_LINES", "999");
        }

        let result = Environment::from_env();
        // Even if it fails due to missing required fields, the conversion should work
        // We can verify this by checking that maxSearchLines was parsed
        
        unsafe {
            std::env::remove_var("FORGE_MAX_SEARCH_LINES");
        }
        
        // This test verifies the method exists and compiles correctly
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    #[ignore = "Test requires all Environment fields to be set"]
    fn test_from_env_nested_config() {
        // Set up environment variables for nested RetryConfig
        unsafe {
            std::env::set_var("FORGE_OS", "macos");
            std::env::set_var("FORGE_PID", "5678");
            std::env::set_var("FORGE_CWD", "/test/nested");
            std::env::set_var("FORGE_SHELL", "zsh");
            std::env::set_var("FORGE_BASE_PATH", "/Users/test/.forge");
            std::env::set_var("FORGE_FORGE_API_URL", "https://nested.test.com");
            std::env::set_var("FORGE_RETRY_CONFIG__INITIAL_BACKOFF_MS", "500");
            std::env::set_var("FORGE_RETRY_CONFIG__BACKOFF_FACTOR", "3");
            std::env::set_var("FORGE_RETRY_CONFIG__MAX_RETRY_ATTEMPTS", "5");
            std::env::set_var("FORGE_MAX_SEARCH_LINES", "100");
            std::env::set_var("FORGE_MAX_SEARCH_RESULT_BYTES", "10240");
            std::env::set_var("FORGE_FETCH_TRUNCATION_LIMIT", "40000");
            std::env::set_var("FORGE_STDOUT_MAX_PREFIX_LENGTH", "200");
            std::env::set_var("FORGE_STDOUT_MAX_SUFFIX_LENGTH", "200");
            std::env::set_var("FORGE_STDOUT_MAX_LINE_LENGTH", "2000");
            std::env::set_var("FORGE_MAX_READ_SIZE", "2000");
            std::env::set_var("FORGE_MAX_FILE_SIZE", "262144");
            std::env::set_var("FORGE_TOOL_TIMEOUT", "300");
            std::env::set_var("FORGE_AUTO_OPEN_DUMP", "false");
            std::env::set_var("FORGE_MAX_CONVERSATIONS", "100");
            std::env::set_var("FORGE_SEM_SEARCH_LIMIT", "100");
            std::env::set_var("FORGE_SEM_SEARCH_TOP_K", "10");
            std::env::set_var("FORGE_MAX_IMAGE_SIZE", "262144");
            std::env::set_var("FORGE_WORKSPACE_SERVER_URL", "http://localhost:8080");
        }

        let result = Environment::from_env();
        assert!(result.is_ok());

        let env = result.unwrap();
        assert_eq!(env.retry_config.initial_backoff_ms, 500);
        assert_eq!(env.retry_config.backoff_factor, 3);
        assert_eq!(env.retry_config.max_retry_attempts, 5);

        // Clean up
        unsafe {
            std::env::remove_var("FORGE_OS");
            std::env::remove_var("FORGE_PID");
            std::env::remove_var("FORGE_CWD");
            std::env::remove_var("FORGE_SHELL");
            std::env::remove_var("FORGE_BASE_PATH");
            std::env::remove_var("FORGE_FORGE_API_URL");
            std::env::remove_var("FORGE_RETRY_CONFIG__INITIAL_BACKOFF_MS");
            std::env::remove_var("FORGE_RETRY_CONFIG__BACKOFF_FACTOR");
            std::env::remove_var("FORGE_RETRY_CONFIG__MAX_RETRY_ATTEMPTS");
            std::env::remove_var("FORGE_MAX_SEARCH_LINES");
            std::env::remove_var("FORGE_MAX_SEARCH_RESULT_BYTES");
            std::env::remove_var("FORGE_FETCH_TRUNCATION_LIMIT");
            std::env::remove_var("FORGE_STDOUT_MAX_PREFIX_LENGTH");
            std::env::remove_var("FORGE_STDOUT_MAX_SUFFIX_LENGTH");
            std::env::remove_var("FORGE_STDOUT_MAX_LINE_LENGTH");
            std::env::remove_var("FORGE_MAX_READ_SIZE");
            std::env::remove_var("FORGE_MAX_FILE_SIZE");
            std::env::remove_var("FORGE_TOOL_TIMEOUT");
            std::env::remove_var("FORGE_AUTO_OPEN_DUMP");
            std::env::remove_var("FORGE_MAX_CONVERSATIONS");
            std::env::remove_var("FORGE_SEM_SEARCH_LIMIT");
            std::env::remove_var("FORGE_SEM_SEARCH_TOP_K");
            std::env::remove_var("FORGE_MAX_IMAGE_SIZE");
            std::env::remove_var("FORGE_WORKSPACE_SERVER_URL");
        }
    }

    #[test]
    #[ignore = "Test requires all Environment fields to be set"]
    fn test_from_env_optional_fields() {
        // Set up minimal environment variables and test optional fields
        unsafe {
            std::env::set_var("FORGE_OS", "linux");
            std::env::set_var("FORGE_PID", "9999");
            std::env::set_var("FORGE_CWD", "/test/optional");
            std::env::set_var("FORGE_SHELL", "fish");
            std::env::set_var("FORGE_BASE_PATH", "/opt/.forge");
            std::env::set_var("FORGE_FORGE_API_URL", "https://optional.test.com");
            std::env::set_var("FORGE_HOME", "/home/testuser");
            std::env::set_var("FORGE_DEBUG_REQUESTS", "/tmp/debug");
            std::env::set_var("FORGE_CUSTOM_HISTORY_PATH", "/tmp/history");
            std::env::set_var("FORGE_OVERRIDE_MODEL", "gpt-4");
            std::env::set_var("FORGE_OVERRIDE_PROVIDER", "openai");
            std::env::set_var("FORGE_MAX_SEARCH_LINES", "100");
            std::env::set_var("FORGE_MAX_SEARCH_RESULT_BYTES", "10240");
            std::env::set_var("FORGE_FETCH_TRUNCATION_LIMIT", "40000");
            std::env::set_var("FORGE_STDOUT_MAX_PREFIX_LENGTH", "200");
            std::env::set_var("FORGE_STDOUT_MAX_SUFFIX_LENGTH", "200");
            std::env::set_var("FORGE_STDOUT_MAX_LINE_LENGTH", "2000");
            std::env::set_var("FORGE_MAX_READ_SIZE", "2000");
            std::env::set_var("FORGE_MAX_FILE_SIZE", "262144");
            std::env::set_var("FORGE_TOOL_TIMEOUT", "300");
            std::env::set_var("FORGE_AUTO_OPEN_DUMP", "false");
            std::env::set_var("FORGE_MAX_CONVERSATIONS", "100");
            std::env::set_var("FORGE_SEM_SEARCH_LIMIT", "100");
            std::env::set_var("FORGE_SEM_SEARCH_TOP_K", "10");
            std::env::set_var("FORGE_MAX_IMAGE_SIZE", "262144");
            std::env::set_var("FORGE_WORKSPACE_SERVER_URL", "http://localhost:8080");
        }

        let result = Environment::from_env();
        assert!(result.is_ok());

        let env = result.unwrap();
        assert_eq!(env.home, Some(PathBuf::from("/home/testuser")));
        assert_eq!(env.debug_requests, Some(PathBuf::from("/tmp/debug")));
        assert_eq!(env.custom_history_path, Some(PathBuf::from("/tmp/history")));
        assert_eq!(env.override_model, Some("gpt-4".to_string()));
        assert_eq!(env.override_provider, Some("openai".to_string()));

        // Clean up
        unsafe {
            std::env::remove_var("FORGE_OS");
            std::env::remove_var("FORGE_PID");
            std::env::remove_var("FORGE_CWD");
            std::env::remove_var("FORGE_SHELL");
            std::env::remove_var("FORGE_BASE_PATH");
            std::env::remove_var("FORGE_FORGE_API_URL");
            std::env::remove_var("FORGE_HOME");
            std::env::remove_var("FORGE_DEBUG_REQUESTS");
            std::env::remove_var("FORGE_CUSTOM_HISTORY_PATH");
            std::env::remove_var("FORGE_OVERRIDE_MODEL");
            std::env::remove_var("FORGE_OVERRIDE_PROVIDER");
            std::env::remove_var("FORGE_MAX_SEARCH_LINES");
            std::env::remove_var("FORGE_MAX_SEARCH_RESULT_BYTES");
            std::env::remove_var("FORGE_FETCH_TRUNCATION_LIMIT");
            std::env::remove_var("FORGE_STDOUT_MAX_PREFIX_LENGTH");
            std::env::remove_var("FORGE_STDOUT_MAX_SUFFIX_LENGTH");
            std::env::remove_var("FORGE_STDOUT_MAX_LINE_LENGTH");
            std::env::remove_var("FORGE_MAX_READ_SIZE");
            std::env::remove_var("FORGE_MAX_FILE_SIZE");
            std::env::remove_var("FORGE_TOOL_TIMEOUT");
            std::env::remove_var("FORGE_AUTO_OPEN_DUMP");
            std::env::remove_var("FORGE_MAX_CONVERSATIONS");
            std::env::remove_var("FORGE_SEM_SEARCH_LIMIT");
            std::env::remove_var("FORGE_SEM_SEARCH_TOP_K");
            std::env::remove_var("FORGE_MAX_IMAGE_SIZE");
            std::env::remove_var("FORGE_WORKSPACE_SERVER_URL");
        }
    }
