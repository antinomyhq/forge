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
#[serde(rename_all = "snake_case")]
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
    /// 1. Embedded JSON config (`env.json` in the crate root, compiled into
    ///    binary)
    /// 2. Environment variables prefixed with `FORGE_` (highest priority)
    ///
    /// Environment variables will override values from the embedded JSON
    /// config. Uses double underscore (`__`) as a separator for nested
    /// configurations.
    ///
    /// # Examples of environment variables:
    /// - `FORGE_OS` -> `os`
    /// - `FORGE_PID` -> `pid`
    /// - `FORGE_CWD` -> `cwd`
    /// - `FORGE_RETRY_CONFIG__INITIAL_BACKOFF_MS` ->
    ///   `retryConfig.initialBackoffMs`
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
            .add_source(config::File::from_str(
                DEFAULT_CONFIG,
                config::FileFormat::Json,
            ))
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
    fn test_agent_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.agent_path();
        let expected = PathBuf::from("/home/user/.forge/agents");

        assert_eq!(actual, expected);
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
    fn test_command_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.command_path();
        let expected = PathBuf::from("/home/user/.forge/commands");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_command_cwd_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.cwd(PathBuf::from("/current/working/dir"));

        let actual = fixture.command_cwd_path();
        let expected = PathBuf::from("/current/working/dir/.forge/commands");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_log_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.log_path();
        let expected = PathBuf::from("/home/user/.forge/logs");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_history_path() {
        let mut fixture: Environment = Faker.fake();
        fixture.base_path = PathBuf::from("/home/user/.forge");
        fixture.custom_history_path = None;

        let actual = fixture.history_path();
        let expected = PathBuf::from("/home/user/.forge/.forge_history");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_history_path_with_custom_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture
            .base_path(PathBuf::from("/home/user/.forge"))
            .custom_history_path(PathBuf::from("/custom/history"));

        let actual = fixture.history_path();
        let expected = PathBuf::from("/custom/history");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_snapshot_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.snapshot_path();
        let expected = PathBuf::from("/home/user/.forge/snapshots");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_mcp_user_config() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.mcp_user_config();
        let expected = PathBuf::from("/home/user/.forge/.mcp.json");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_templates() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.templates();
        let expected = PathBuf::from("/home/user/.forge/templates");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_permissions_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.permissions_path();
        let expected = PathBuf::from("/home/user/.forge/permissions.yaml");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_mcp_local_config() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.cwd(PathBuf::from("/projects/my-app"));

        let actual = fixture.mcp_local_config();
        let expected = PathBuf::from("/projects/my-app/.mcp.json");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_version() {
        let fixture: Environment = Faker.fake();

        let actual = fixture.version();

        assert!(!actual.is_empty());
    }

    #[test]
    fn test_app_config() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.app_config();
        let expected = PathBuf::from("/home/user/.forge/.config.json");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_database_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.database_path();
        let expected = PathBuf::from("/home/user/.forge/.forge.db");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cache_dir() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.cache_dir();
        let expected = PathBuf::from("/home/user/.forge/cache");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_workspace_hash() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.cwd(PathBuf::from("/projects/my-app"));

        let actual = fixture.workspace_hash();

        assert!(actual.id() > 0);
    }

    #[test]
    fn test_workspace_hash_consistency() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.cwd(PathBuf::from("/projects/my-app"));

        let hash1 = fixture.workspace_hash();
        let hash2 = fixture.workspace_hash();

        assert_eq!(hash1.id(), hash2.id());
    }

    #[test]
    fn test_from_env_loads_default_config() {
        let env = Environment::from_env().unwrap();

        // Verify default values come from embedded JSON config
        assert_eq!(env.max_search_lines, 200);
        assert_eq!(env.http.connect_timeout, 30);
        assert_eq!(env.http.read_timeout, 900);
        assert_eq!(env.os, "macos");
    }
}
