use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::{DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra};
use forge_domain::{AgentId, AgentSource, PluginRepository, Template};
use gray_matter::Matter;
use gray_matter::engine::YAML;

use crate::agent_definition::AgentDefinition;

/// Infrastructure implementation for loading agent definitions from multiple
/// sources:
/// 1. Built-in agents (embedded in the application)
/// 2. Plugin agents (from each enabled plugin's `agents_paths`)
/// 3. Global custom agents (from ~/.forge/agents/ directory)
/// 4. Project-local agents (from .forge/agents/ directory in current working
///    directory)
///
/// ## Agent Precedence
/// When agents have duplicate IDs across different sources, the precedence
/// order is: **CWD (project-local) > Global custom > Plugin > Built-in**
///
/// This means project-local agents can override global agents, which can
/// override plugin agents, and all of those can override built-in agents.
///
/// ## Directory Resolution
/// - **Built-in agents**: Embedded in application binary
/// - **Plugin agents**: `<plugin-root>/agents/*.md`, loaded only for plugins
///   whose `enabled` flag is `true`. Plugin agents are namespaced as
///   `{plugin_name}:{agent_id}` to avoid collisions across plugins.
/// - **Global agents**: `~/forge/agents/*.md`
/// - **CWD agents**: `./.forge/agents/*.md` (relative to current working
///   directory)
///
/// Missing directories are handled gracefully and don't prevent loading from
/// other sources.
pub struct ForgeAgentRepository<I> {
    infra: Arc<I>,
    plugin_repository: Option<Arc<dyn PluginRepository>>,
}

impl<I> ForgeAgentRepository<I> {
    /// Construct an agent repository that also loads plugin-provided agents
    /// from the supplied [`PluginRepository`]. This is the production entry
    /// point used by `ForgeRepo::new`.
    pub fn new(infra: Arc<I>, plugin_repository: Arc<dyn PluginRepository>) -> Self {
        Self { infra, plugin_repository: Some(plugin_repository) }
    }

    /// Construct an agent repository with no plugin loader wired in. Only
    /// used by unit tests that do not care about plugin-sourced agents.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_without_plugins(infra: Arc<I>) -> Self {
        Self { infra, plugin_repository: None }
    }
}

impl<I: FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra> ForgeAgentRepository<I> {
    /// Load all agent definitions from all available sources with conflict
    /// resolution.
    pub(crate) async fn load_agents(&self) -> anyhow::Result<Vec<AgentDefinition>> {
        self.load_all_agents().await
    }

    /// Load all agent definitions from all available sources
    async fn load_all_agents(&self) -> anyhow::Result<Vec<AgentDefinition>> {
        // Load built-in agents (no path - will display as "BUILT IN")
        let mut agents = self.init_default().await?;

        // Plugin agents sit between built-in and user-global custom.
        let plugin_agents = self.load_plugin_agents().await;
        agents.extend(plugin_agents);

        // Load custom agents from global directory
        let dir = self.infra.get_environment().agent_path();
        let mut custom_agents = self.init_agent_dir(&dir).await?;
        for agent in &mut custom_agents {
            agent.source = AgentSource::GlobalUser;
        }
        agents.extend(custom_agents);

        // Load custom agents from CWD
        let dir = self.infra.get_environment().agent_cwd_path();
        let mut cwd_agents = self.init_agent_dir(&dir).await?;
        for agent in &mut cwd_agents {
            agent.source = AgentSource::ProjectCwd;
        }
        agents.extend(cwd_agents);

        // Handle agent ID conflicts by keeping the last occurrence
        // This gives precedence order: CWD > Global Custom > Plugin > Built-in
        Ok(resolve_agent_conflicts(agents))
    }

    async fn init_default(&self) -> anyhow::Result<Vec<AgentDefinition>> {
        parse_agent_iter(
            [
                ("forge", include_str!("agents/forge.md")),
                ("muse", include_str!("agents/muse.md")),
                ("sage", include_str!("agents/sage.md")),
            ]
            .into_iter()
            .map(|(name, content)| (name.to_string(), content.to_string())),
        )
    }

    async fn init_agent_dir(&self, dir: &std::path::Path) -> anyhow::Result<Vec<AgentDefinition>> {
        if !self.infra.exists(dir).await? {
            return Ok(vec![]);
        }

        // Use DirectoryReaderInfra to read all .md files in parallel
        let files = self
            .infra
            .read_directory_files(dir, Some("*.md"))
            .await
            .with_context(|| format!("Failed to read agents from: {}", dir.display()))?;

        let mut agents = Vec::new();
        for (path, content) in files {
            let mut agent = parse_agent_file(&content)
                .with_context(|| format!("Failed to parse agent: {}", path.display()))?;

            // Store the file path
            agent.path = Some(path.display().to_string());
            agents.push(agent);
        }

        Ok(agents)
    }

    /// Loads all plugin-provided agents from every enabled plugin returned
    /// by the injected [`PluginRepository`]. Returns an empty vector when
    /// no plugin repository is wired in (used by unit tests).
    async fn load_plugin_agents(&self) -> Vec<AgentDefinition> {
        let Some(plugin_repo) = self.plugin_repository.as_ref() else {
            return Vec::new();
        };

        let plugins = match plugin_repo.load_plugins().await {
            Ok(plugins) => plugins,
            Err(err) => {
                tracing::warn!("Failed to enumerate plugins for agent loading: {err:#}");
                return Vec::new();
            }
        };

        let mut all = Vec::new();
        for plugin in plugins.into_iter().filter(|p| p.enabled) {
            for agents_dir in &plugin.agents_paths {
                match self
                    .load_plugin_agents_from_dir(agents_dir, &plugin.name)
                    .await
                {
                    Ok(loaded) => all.extend(loaded),
                    Err(err) => {
                        tracing::warn!(
                            "Failed to load plugin agents from {}: {err:#}",
                            agents_dir.display()
                        );
                    }
                }
            }
        }

        all
    }

    /// Walks a plugin `agents_dir` (one level, `.md` files only), parses each
    /// file as an [`AgentDefinition`], and namespaces the resulting agent id
    /// as `{plugin_name}:{original_id}`. Every returned definition is tagged
    /// with [`AgentSource::Plugin`].
    async fn load_plugin_agents_from_dir(
        &self,
        dir: &std::path::Path,
        plugin_name: &str,
    ) -> anyhow::Result<Vec<AgentDefinition>> {
        if !self.infra.exists(dir).await? {
            return Ok(vec![]);
        }

        let files = self
            .infra
            .read_directory_files(dir, Some("*.md"))
            .await
            .with_context(|| format!("Failed to read plugin agents from: {}", dir.display()))?;

        let mut agents = Vec::new();
        for (path, content) in files {
            let mut agent = match parse_agent_file(&content) {
                Ok(agent) => agent,
                Err(err) => {
                    tracing::warn!("Failed to parse plugin agent {}: {err:#}", path.display());
                    continue;
                }
            };

            // Namespace plugin agent ids as `{plugin_name}:{original_id}` so
            // multiple plugins cannot collide on the same `id` field in their
            // frontmatter.
            let namespaced = format!("{plugin_name}:{}", agent.id.as_str());
            agent.id = AgentId::new(namespaced);
            agent.path = Some(path.display().to_string());
            agent.source = AgentSource::Plugin { plugin_name: plugin_name.to_string() };
            agents.push(agent);
        }

        Ok(agents)
    }
}

/// Implementation function for resolving agent ID conflicts by keeping the last
/// occurrence. This implements the precedence order: CWD Custom > Global Custom
/// > Plugin > Built-in
fn resolve_agent_conflicts(agents: Vec<AgentDefinition>) -> Vec<AgentDefinition> {
    use std::collections::HashMap;

    // Use HashMap to deduplicate by agent ID, keeping the last occurrence
    let mut agent_map: HashMap<String, AgentDefinition> = HashMap::new();

    for agent in agents {
        agent_map.insert(agent.id.to_string(), agent);
    }

    // Convert back to vector (order is not guaranteed but doesn't matter for the
    // service)
    agent_map.into_values().collect()
}

fn parse_agent_iter<I, Path: AsRef<str>, Content: AsRef<str>>(
    contents: I,
) -> anyhow::Result<Vec<AgentDefinition>>
where
    I: Iterator<Item = (Path, Content)>,
{
    let mut agents = vec![];

    for (name, content) in contents {
        let agent = parse_agent_file(content.as_ref())
            .with_context(|| format!("Failed to parse agent: {}", name.as_ref()))?;

        agents.push(agent);
    }

    Ok(agents)
}

/// Parse raw content into an AgentDefinition with YAML frontmatter
fn parse_agent_file(content: &str) -> Result<AgentDefinition> {
    // Parse the frontmatter using gray_matter with type-safe deserialization
    let gray_matter = Matter::<YAML>::new();
    let result = gray_matter.parse::<AgentDefinition>(content)?;

    // Extract the frontmatter
    let agent = result
        .data
        .context("Empty system prompt content")?
        .system_prompt(Template::new(result.content));

    Ok(agent)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_config::ForgeConfig;
    use forge_domain::{LoadedPlugin, PluginLoadResult, PluginManifest, PluginSource};
    use forge_infra::ForgeInfra;
    use pretty_assertions::assert_eq;

    use super::*;

    /// Test-only in-memory [`PluginRepository`] that returns a fixed list of
    /// loaded plugins. Mirrors the helper used in `skill.rs` tests so the
    /// agent loader can be exercised without touching the real plugin
    /// discovery pipeline.
    struct MockPluginRepository {
        plugins: Vec<LoadedPlugin>,
    }

    #[async_trait::async_trait]
    impl PluginRepository for MockPluginRepository {
        async fn load_plugins(&self) -> anyhow::Result<Vec<LoadedPlugin>> {
            Ok(self.plugins.clone())
        }

        async fn load_plugins_with_errors(&self) -> anyhow::Result<PluginLoadResult> {
            Ok(PluginLoadResult::new(self.plugins.clone(), Vec::new()))
        }
    }

    fn fixture_plugin(name: &str, enabled: bool, agents_path: PathBuf) -> LoadedPlugin {
        LoadedPlugin {
            name: name.to_string(),
            manifest: PluginManifest { name: Some(name.to_string()), ..Default::default() },
            path: PathBuf::from(format!("/fake/{name}")),
            source: PluginSource::Global,
            enabled,
            is_builtin: false,
            commands_paths: Vec::new(),
            agents_paths: vec![agents_path],
            skills_paths: Vec::new(),
            mcp_servers: None,
        }
    }

    fn fixture_agent_repo_with_plugins(
        plugins: Vec<LoadedPlugin>,
    ) -> ForgeAgentRepository<ForgeInfra> {
        let config = ForgeConfig::read().unwrap_or_default();
        let services_url = config.services_url.parse().unwrap();
        let infra = Arc::new(ForgeInfra::new(
            std::env::current_dir().unwrap(),
            config,
            services_url,
        ));
        let plugin_repo: Arc<dyn PluginRepository> = Arc::new(MockPluginRepository { plugins });
        ForgeAgentRepository::new(infra, plugin_repo)
    }

    #[tokio::test]
    async fn test_parse_basic_agent() {
        let content = forge_test_kit::fixture!("/src/fixtures/agents/basic.md").await;

        let actual = parse_agent_file(&content).unwrap();

        assert_eq!(actual.id.as_str(), "test-basic");
        assert_eq!(actual.title.as_ref().unwrap(), "Basic Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "A simple test agent for basic functionality"
        );
        assert_eq!(
            actual.system_prompt.as_ref().unwrap().template,
            "This is a basic test agent used for testing fundamental functionality."
        );
    }

    #[tokio::test]
    async fn test_parse_advanced_agent() {
        let content = forge_test_kit::fixture!("/src/fixtures/agents/advanced.md").await;

        let actual = parse_agent_file(&content).unwrap();

        assert_eq!(actual.id.as_str(), "test-advanced");
        assert_eq!(actual.title.as_ref().unwrap(), "Advanced Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "An advanced test agent with full configuration"
        );
    }

    #[tokio::test]
    async fn test_load_plugin_agents_namespaces_and_tags_source() {
        let agents_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/plugin_agents");
        let plugin = fixture_plugin("demo", true, agents_dir);
        let repo = fixture_agent_repo_with_plugins(vec![plugin]);

        let actual = repo.load_plugin_agents().await;

        // Two fixture agents should be discovered.
        assert_eq!(actual.len(), 2);

        // Every loaded agent must be namespaced with the plugin name and
        // tagged with AgentSource::Plugin.
        for agent in &actual {
            assert!(
                agent.id.as_str().starts_with("demo:"),
                "expected namespaced id, got {}",
                agent.id.as_str()
            );
            assert_eq!(
                agent.source,
                AgentSource::Plugin { plugin_name: "demo".to_string() }
            );
            assert!(agent.path.is_some());
        }

        // Specific expected namespaced ids from the fixture files.
        assert!(actual.iter().any(|a| a.id.as_str() == "demo:reviewer"));
        assert!(actual.iter().any(|a| a.id.as_str() == "demo:deployer"));
    }

    #[tokio::test]
    async fn test_load_plugin_agents_skips_disabled_plugins() {
        let agents_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/plugin_agents");
        let plugin = fixture_plugin("demo", false, agents_dir);
        let repo = fixture_agent_repo_with_plugins(vec![plugin]);

        let actual = repo.load_plugin_agents().await;
        assert!(
            actual.is_empty(),
            "disabled plugin agents should be skipped"
        );
    }

    #[tokio::test]
    async fn test_load_plugin_agents_handles_missing_agents_dir() {
        let missing = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fixtures/definitely-does-not-exist");
        let plugin = fixture_plugin("demo", true, missing);
        let repo = fixture_agent_repo_with_plugins(vec![plugin]);

        let actual = repo.load_plugin_agents().await;
        assert!(actual.is_empty());
    }
}
