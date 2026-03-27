use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::str::FromStr;

use derive_more::Display;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    CommitConfig, Compact, HttpConfig, MaxTokens, ModelId, ProviderId, RetryConfig, SuggestConfig,
    Temperature, TopK, TopP, Update,
};

/// Domain-level session configuration pairing a provider with a model.
///
/// Used inside [`Environment`] to represent the active session, decoupled from
/// the on-disk configuration format.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct SessionConfig {
    /// The active provider ID (e.g. `"anthropic"`).
    pub provider_id: Option<String>,
    /// The model ID to use with this provider.
    pub model_id: Option<String>,
}

/// All discrete mutations that can be applied to the application configuration.
///
/// Instead of replacing the entire config, callers describe exactly which field
/// they want to change. Implementations receive a list of operations, apply
/// each in order, and persist the result atomically.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigOperation {
    /// Set the active provider.
    SetProvider(ProviderId),
    /// Set the model for the given provider.
    SetModel(ProviderId, ModelId),
    /// Set the commit-message generation configuration.
    SetCommitConfig(CommitConfig),
    /// Set the shell-command suggestion configuration.
    SetSuggestConfig(SuggestConfig),
}

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Debug, Setters, Clone, PartialEq, Serialize, Deserialize, fake::Dummy)]
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
    /// Maximum characters per line for file read operations
    /// Controlled by FORGE_MAX_LINE_LENGTH environment variable.
    pub max_line_length: usize,
    /// Maximum number of lines to read from a file
    pub max_read_size: u64,
    /// Maximum number of files that can be read in a single batch operation.
    /// Controlled by FORGE_MAX_READ_BATCH_SIZE environment variable.
    pub max_file_read_batch_size: usize,
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
    pub service_url: Url,
    /// Maximum number of file extensions to include in the system prompt.
    /// Controlled by FORGE_MAX_EXTENSIONS environment variable.
    pub max_extensions: usize,
    /// Format for automatically creating a dump when a task is completed.
    /// Controlled by FORGE_AUTO_DUMP environment variable.
    /// Set to "json" (or "true"/"1"/"yes") for JSON, "html" for HTML, or
    /// unset/other to disable.
    pub auto_dump: Option<AutoDumpFormat>,
    /// Maximum number of files read concurrently in parallel operations.
    /// Controlled by FORGE_PARALLEL_FILE_READS environment variable.
    /// Caps the `buffer_unordered` concurrency to avoid EMFILE errors.
    pub parallel_file_reads: usize,
    /// TTL in seconds for the model API list cache.
    /// Controlled by FORGE_MODEL_CACHE_TTL environment variable.
    pub model_cache_ttl: u64,

    // --- User configuration fields (from ForgeConfig) ---
    /// The active session (provider + model).
    #[dummy(default)]
    pub session: Option<SessionConfig>,
    /// Provider and model for commit message generation.
    #[dummy(default)]
    pub commit: Option<SessionConfig>,
    /// Provider and model for shell command suggestion generation.
    #[dummy(default)]
    pub suggest: Option<SessionConfig>,
    /// Whether the application is running in restricted mode.
    /// When true, tool execution requires explicit permission grants.
    pub is_restricted: bool,

    /// Whether tool use is supported in the current environment.
    /// When false, tool calls are disabled regardless of agent configuration.
    pub tool_supported: bool,

    // --- Workflow configuration fields ---
    /// Output randomness for all agents; lower values are deterministic, higher
    /// values are creative (0.0–2.0).
    #[dummy(default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<Temperature>,

    /// Nucleus sampling threshold for all agents; limits token selection to the
    /// top cumulative probability mass (0.0–1.0).
    #[dummy(default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<TopP>,

    /// Top-k vocabulary cutoff for all agents; restricts sampling to the k
    /// highest-probability tokens (1–1000).
    #[dummy(default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<TopK>,

    /// Maximum tokens the model may generate per response for all agents
    /// (1–100,000).
    #[dummy(default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<MaxTokens>,

    /// Maximum tool failures per turn before the orchestrator forces
    /// completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_failure_per_turn: Option<usize>,

    /// Maximum number of requests that can be made in a single turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_requests_per_turn: Option<usize>,

    /// Context compaction settings applied to all agents.
    #[dummy(default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact: Option<Compact>,

    /// Configuration for automatic forge updates.
    #[dummy(default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates: Option<Update>,
}

/// The output format used when auto-dumping a conversation on task completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, fake::Dummy)]
#[serde(rename_all = "camelCase")]
pub enum AutoDumpFormat {
    /// Dump as a JSON file.
    Json,
    /// Dump as an HTML file.
    Html,
}

impl FromStr for AutoDumpFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "html" => Ok(AutoDumpFormat::Html),
            "json" | "true" | "1" | "yes" => Ok(AutoDumpFormat::Json),
            _ => Err(()),
        }
    }
}

impl Environment {
    /// Applies a single [`ConfigOperation`] to this environment in-place.
    pub fn apply_op(&mut self, op: ConfigOperation) {
        match op {
            ConfigOperation::SetProvider(provider_id) => {
                let pid = provider_id.as_ref().to_string();
                self.session = Some(match self.session.take() {
                    Some(sc) => sc.provider_id(pid),
                    None => SessionConfig::default().provider_id(pid),
                });
            }
            ConfigOperation::SetModel(provider_id, model_id) => {
                let pid = provider_id.as_ref().to_string();
                let mid = model_id.to_string();
                self.session = Some(match self.session.take() {
                    Some(sc) if sc.provider_id.as_deref() == Some(&pid) => sc.model_id(mid),
                    _ => SessionConfig::default().provider_id(pid).model_id(mid),
                });
            }
            ConfigOperation::SetCommitConfig(commit) => {
                self.commit =
                    commit
                        .provider
                        .as_ref()
                        .zip(commit.model.as_ref())
                        .map(|(pid, mid)| {
                            SessionConfig::default()
                                .provider_id(pid.as_ref().to_string())
                                .model_id(mid.to_string())
                        });
            }
            ConfigOperation::SetSuggestConfig(suggest) => {
                self.suggest = Some(
                    SessionConfig::default()
                        .provider_id(suggest.provider.as_ref().to_string())
                        .model_id(suggest.model.to_string()),
                );
            }
        }
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

    pub fn agent_path(&self) -> PathBuf {
        self.base_path.join("agents")
    }
    pub fn agent_cwd_path(&self) -> PathBuf {
        self.cwd.join(".forge/agents")
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

    /// Returns the global commands directory path (base_path/commands)
    pub fn command_path(&self) -> PathBuf {
        self.base_path.join("commands")
    }

    /// Returns the project-local commands directory path (.forge/commands)
    pub fn command_path_local(&self) -> PathBuf {
        self.cwd.join(".forge/commands")
    }

    /// Returns the global AGENTS.md path (base_path/AGENTS.md)
    pub fn global_agentsmd_path(&self) -> PathBuf {
        self.base_path.join("AGENTS.md")
    }

    /// Returns the project-local AGENTS.md path (cwd/AGENTS.md)
    pub fn local_agentsmd_path(&self) -> PathBuf {
        self.cwd.join("AGENTS.md")
    }

    /// Returns the plans directory path relative to the current working
    /// directory (cwd/plans)
    pub fn plans_path(&self) -> PathBuf {
        self.cwd.join("plans")
    }

    /// Returns the path to the custom provider configuration file
    /// (base_path/provider.json)
    pub fn provider_config_path(&self) -> PathBuf {
        self.base_path.join("provider.json")
    }

    /// Returns the path to the credentials file where provider API keys are
    /// stored
    pub fn credentials_path(&self) -> PathBuf {
        self.base_path.join(".credentials.json")
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

    fn fixture_env() -> Environment {
        Faker.fake()
    }

    #[test]
    fn test_apply_op_set_provider_creates_session_when_absent() {
        let mut fixture = fixture_env();
        fixture.apply_op(ConfigOperation::SetProvider(ProviderId::from(
            "anthropic".to_string(),
        )));
        let expected = SessionConfig::default().provider_id("anthropic".to_string());
        assert_eq!(fixture.session, Some(expected));
    }

    #[test]
    fn test_apply_op_set_provider_updates_existing_session_keeping_model() {
        let mut fixture = fixture_env();
        fixture.session = Some(
            SessionConfig::default()
                .provider_id("openai".to_string())
                .model_id("gpt-4".to_string()),
        );
        fixture.apply_op(ConfigOperation::SetProvider(ProviderId::from(
            "anthropic".to_string(),
        )));
        let expected = SessionConfig::default()
            .provider_id("anthropic".to_string())
            .model_id("gpt-4".to_string());
        assert_eq!(fixture.session, Some(expected));
    }

    #[test]
    fn test_apply_op_set_model_for_matching_provider_updates_model() {
        let mut fixture = fixture_env();
        fixture.session = Some(
            SessionConfig::default()
                .provider_id("openai".to_string())
                .model_id("gpt-3.5".to_string()),
        );
        fixture.apply_op(ConfigOperation::SetModel(
            ProviderId::from("openai".to_string()),
            ModelId::new("gpt-4"),
        ));
        let expected = SessionConfig::default()
            .provider_id("openai".to_string())
            .model_id("gpt-4".to_string());
        assert_eq!(fixture.session, Some(expected));
    }

    #[test]
    fn test_apply_op_set_model_for_different_provider_replaces_session() {
        let mut fixture = fixture_env();
        fixture.session = Some(
            SessionConfig::default()
                .provider_id("openai".to_string())
                .model_id("gpt-4".to_string()),
        );
        fixture.apply_op(ConfigOperation::SetModel(
            ProviderId::from("anthropic".to_string()),
            ModelId::new("claude-3"),
        ));
        let expected = SessionConfig::default()
            .provider_id("anthropic".to_string())
            .model_id("claude-3".to_string());
        assert_eq!(fixture.session, Some(expected));
    }

    #[test]
    fn test_apply_op_set_commit_config() {
        let mut fixture = fixture_env();
        let commit = CommitConfig::default()
            .provider(ProviderId::from("openai".to_string()))
            .model(ModelId::new("gpt-4o"));
        fixture.apply_op(ConfigOperation::SetCommitConfig(commit));
        let expected = SessionConfig::default()
            .provider_id("openai".to_string())
            .model_id("gpt-4o".to_string());
        assert_eq!(fixture.commit, Some(expected));
    }

    #[test]
    fn test_apply_op_set_suggest_config() {
        let mut fixture = fixture_env();
        let suggest = SuggestConfig {
            provider: ProviderId::from("anthropic".to_string()),
            model: ModelId::new("claude-3-haiku"),
        };
        fixture.apply_op(ConfigOperation::SetSuggestConfig(suggest));
        let expected = SessionConfig::default()
            .provider_id("anthropic".to_string())
            .model_id("claude-3-haiku".to_string());
        assert_eq!(fixture.suggest, Some(expected));
    }

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

    #[test]
    fn test_command_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.command_path();
        let expected = PathBuf::from("/home/user/.forge/commands");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_command_path_local() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.cwd(PathBuf::from("/projects/my-app"));

        let actual = fixture.command_path_local();
        let expected = PathBuf::from("/projects/my-app/.forge/commands");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_command_paths_independent() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture
            .cwd(PathBuf::from("/projects/my-app"))
            .base_path(PathBuf::from("/home/user/.forge"));

        let global_path = fixture.command_path();
        let local_path = fixture.command_path_local();

        let expected_global = PathBuf::from("/home/user/.forge/commands");
        let expected_local = PathBuf::from("/projects/my-app/.forge/commands");

        assert_eq!(global_path, expected_global);
        assert_eq!(local_path, expected_local);
        assert_ne!(global_path, local_path);
    }

    #[test]
    fn test_global_agents_md_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.global_agentsmd_path();
        let expected = PathBuf::from("/home/user/.forge/AGENTS.md");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_local_agents_md_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.cwd(PathBuf::from("/projects/my-app"));

        let actual = fixture.local_agentsmd_path();
        let expected = PathBuf::from("/projects/my-app/AGENTS.md");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_plans_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.cwd(PathBuf::from("/projects/my-app"));

        let actual = fixture.plans_path();
        let expected = PathBuf::from("/projects/my-app/plans");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_provider_config_path() {
        let fixture: Environment = Faker.fake();
        let fixture = fixture.base_path(PathBuf::from("/home/user/.forge"));

        let actual = fixture.provider_config_path();
        let expected = PathBuf::from("/home/user/.forge/provider.json");

        assert_eq!(actual, expected);
    }
}
