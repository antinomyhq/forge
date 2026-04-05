use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;

use derive_more::Display;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{Effort, ModelId, ProviderId};

/// Domain-level session configuration pairing a provider with a model.
///
/// Used to represent an active session, decoupled from the on-disk
/// configuration format.
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
    SetCommitConfig(crate::CommitConfig),
    /// Set the shell-command suggestion configuration.
    SetSuggestConfig(crate::SuggestConfig),
    /// Set the reasoning effort level for all agents.
    SetReasoningEffort(Effort),
}

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

/// Represents the minimal runtime environment in which the application is
/// running.
///
/// Contains only the six fields that cannot be sourced from [`ForgeConfig`]:
/// `os`, `pid`, `cwd`, `home`, `shell`, and `base_path`. All configuration
/// values previously carried here are now accessed through
/// `EnvironmentInfra::get_config()`.
#[derive(Debug, Setters, Clone, PartialEq, Serialize, Deserialize, fake::Dummy)]
#[serde(rename_all = "camelCase")]
#[setters(strip_option)]
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
    /// The base path relative to which everything else is stored.
    pub base_path: PathBuf,
}

impl Environment {
    pub fn log_path(&self) -> PathBuf {
        self.base_path.join("logs")
    }

    /// Returns the history file path.
    ///
    /// # Arguments
    /// * `custom_path` - An optional custom path sourced from
    ///   `ForgeConfig::custom_history_path`. When present it overrides the
    ///   default location inside `base_path`.
    pub fn history_path(&self, custom_path: Option<&PathBuf>) -> PathBuf {
        custom_path
            .cloned()
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
