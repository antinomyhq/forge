use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{
    Agent, AgentDefinition, AppConfigRepository, ModelId, ProviderId, ProviderRepository, Template,
};
use forge_app::{
    DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra,
};
use gray_matter::Matter;
use gray_matter::engine::YAML;

/// A service for loading agent definitions from multiple sources:
/// 1. Built-in agents (embedded in the application)
/// 2. Global custom agents (from ~/.forge/agents/ directory)
/// 3. Project-local agents (from .forge/agents/ directory in current working
///    directory)
///
/// ## Agent Precedence
/// When agents have duplicate IDs across different sources, the precedence
/// order is: **CWD (project-local) > Global custom > Built-in**
///
/// This means project-local agents can override global agents, and both can
/// override built-in agents.
///
/// ## Directory Resolution
/// - **Built-in agents**: Embedded in application binary
/// - **Global agents**: `{HOME}/.forge/agents/*.md`
/// - **CWD agents**: `./.forge/agents/*.md` (relative to current working
///   directory)
///
/// Missing directories are handled gracefully and don't prevent loading from
/// other sources.
pub struct AgentLoaderService<F> {
    infra: Arc<F>,

    // Cache is used to maintain the loaded agents
    // for this service instance.
    // So that they could live till user starts a new session.
    cache: tokio::sync::OnceCell<Vec<Agent>>,

    // In-memory storage for the active agent ID
    // This is NOT persisted to disk
    active_agent_id: tokio::sync::RwLock<Option<forge_app::domain::AgentId>>,
}

impl<F> AgentLoaderService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self {
            infra,
            cache: Default::default(),
            active_agent_id: tokio::sync::RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl<
    F: FileReaderInfra
        + FileWriterInfra
        + FileInfoInfra
        + EnvironmentInfra
        + DirectoryReaderInfra
        + AppConfigRepository
        + ProviderRepository,
> forge_app::AgentRegistry for AgentLoaderService<F>
{
    /// Load all agent definitions from all available sources with conflict
    /// resolution.
    ///
    /// This method loads agents from three sources in order:
    /// 1. Built-in agents (always available)
    /// 2. Global custom agents (from ~/.forge/agents/ if directory exists)
    /// 3. Project-local agents (from ./.forge/agents/ if directory exists)
    ///
    /// Duplicate agent IDs are resolved using last-wins strategy, giving
    /// precedence to project-local agents over global agents, and both over
    /// built-in agents.
    async fn get_agents(&self) -> anyhow::Result<Vec<Agent>> {
        // Get default provider using repository
        let app_config = self.infra.get_app_config().await?;
        let default_provider_id = app_config.provider.unwrap_or(ProviderId::Anthropic);

        // Get default model for the provider
        let default_model = app_config
            .model
            .get(&default_provider_id)
            .cloned()
            .unwrap_or_else(|| ModelId::new("claude-3-5-sonnet-20241022"));

        self.cache_or_init(&default_provider_id, &default_model)
            .await
    }

    async fn get_agent(
        &self,
        agent_id: &forge_app::domain::AgentId,
    ) -> anyhow::Result<Option<Agent>> {
        let agents = self.get_agents().await?;
        Ok(agents.into_iter().find(|agent| &agent.id == agent_id))
    }

    async fn get_active_agent(&self) -> anyhow::Result<Option<Agent>> {
        let agent_id = self.active_agent_id.read().await;

        if let Some(ref id) = *agent_id {
            let agents = self.get_agents().await?;
            Ok(agents.into_iter().find(|agent| &agent.id == id))
        } else {
            Ok(None)
        }
    }

    async fn get_active_agent_id(&self) -> anyhow::Result<Option<forge_app::domain::AgentId>> {
        let agent_id = self.active_agent_id.read().await;
        Ok(agent_id.clone())
    }

    async fn set_active_agent_id(
        &self,
        agent_id: forge_app::domain::AgentId,
    ) -> anyhow::Result<()> {
        let mut active_agent = self.active_agent_id.write().await;
        *active_agent = Some(agent_id);
        Ok(())
    }
}

impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    AgentLoaderService<F>
{
    /// Load all agent definitions with caching support
    async fn cache_or_init(
        &self,
        default_provider: &ProviderId,
        default_model: &ModelId,
    ) -> anyhow::Result<Vec<Agent>> {
        // Check if cache exists first
        if let Some(cached) = self.cache.get() {
            return Ok(cached.clone());
        }

        // Initialize and cache
        let agents = self.init(default_provider, default_model).await?;
        let cached = self.cache.get_or_init(|| async { agents.clone() }).await;
        Ok(cached.clone())
    }

    async fn init(
        &self,
        default_provider: &ProviderId,
        default_model: &ModelId,
    ) -> anyhow::Result<Vec<Agent>> {
        // Load built-in agent definitions
        let mut definitions = self.init_default().await?;

        // Load custom agent definitions from global directory
        let dir = self.infra.get_environment().agent_path();
        let custom_definitions = self.init_agent_dir(&dir).await?;
        definitions.extend(custom_definitions);

        // Load custom agent definitions from CWD
        let dir = self.infra.get_environment().agent_cwd_path();
        let cwd_definitions = self.init_agent_dir(&dir).await?;
        definitions.extend(cwd_definitions);

        // Resolve definition conflicts by keeping the last occurrence
        // This gives precedence order: CWD > Global Custom > Built-in
        let definitions = resolve_definition_conflicts(definitions);

        // Convert definitions to agents with default provider/model
        let agents: Vec<Agent> = definitions
            .into_iter()
            .map(|def| {
                let agent_provider = def.provider.unwrap_or(*default_provider);
                let agent_model = def.model.clone().unwrap_or_else(|| default_model.clone());
                def.into_agent(agent_provider, agent_model)
            })
            .collect();

        Ok(agents)
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

        parse_agent_iter(files.into_iter().map(|(path, content)| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            (name, content)
        }))
    }
}

/// Implementation function for resolving agent definition ID conflicts by
/// keeping the last occurrence. This implements the precedence order: CWD
/// Custom > Global Custom > Built-in
fn resolve_definition_conflicts(definitions: Vec<AgentDefinition>) -> Vec<AgentDefinition> {
    use std::collections::HashMap;

    // Use HashMap to deduplicate by agent ID, keeping the last occurrence
    let mut definition_map: HashMap<String, AgentDefinition> = HashMap::new();

    for definition in definitions {
        definition_map.insert(definition.id.to_string(), definition);
    }

    // Convert back to vector (order is not guaranteed but doesn't matter for the
    // service)
    definition_map.into_values().collect()
}

fn parse_agent_iter<I, Path: AsRef<str>, Content: AsRef<str>>(
    contents: I,
) -> anyhow::Result<Vec<AgentDefinition>>
where
    I: Iterator<Item = (Path, Content)>,
{
    let mut definitions = vec![];

    for (name, content) in contents {
        let definition = parse_agent_file(content.as_ref())
            .with_context(|| format!("Failed to parse agent: {}", name.as_ref()))?;

        definitions.push(definition);
    }

    Ok(definitions)
}

/// Parse raw content into an AgentDefinition with YAML frontmatter
fn parse_agent_file(content: &str) -> Result<AgentDefinition> {
    // Parse the frontmatter using gray_matter with type-safe deserialization
    let gray_matter = Matter::<YAML>::new();
    let result = gray_matter.parse::<AgentDefinition>(content)?;

    // Extract the frontmatter
    let definition = result
        .data
        .context("Empty system prompt content")?
        .system_prompt(Template::new(result.content));

    Ok(definition)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_parse_basic_agent() {
        let content = include_str!("fixtures/agents/basic.md");

        let actual = parse_agent_file(content).unwrap();

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
        let content = include_str!("fixtures/agents/advanced.md");

        let actual = parse_agent_file(content).unwrap();

        assert_eq!(actual.id.as_str(), "test-advanced");
        assert_eq!(actual.title.as_ref().unwrap(), "Advanced Test Agent");
        assert_eq!(
            actual.description.as_ref().unwrap(),
            "An advanced test agent with full configuration"
        );
        assert_eq!(
            actual.model.as_ref().unwrap().as_str(),
            "claude-3-5-sonnet-20241022"
        );
        assert_eq!(actual.tool_supported, Some(true));
        assert!(actual.tools.is_some());
        assert_eq!(actual.temperature.as_ref().unwrap().value(), 0.7);
        assert_eq!(actual.top_p.as_ref().unwrap().value(), 0.9);
        assert_eq!(actual.max_tokens.as_ref().unwrap().value(), 2000);
        assert_eq!(actual.max_turns, Some(10));
        assert!(actual.reasoning.is_some());
    }

    #[tokio::test]
    async fn test_parse_invalid_frontmatter() {
        let content = include_str!("fixtures/agents/invalid.md");

        let result = parse_agent_file(content);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_builtin_agents() {
        // Test that all built-in agents parse correctly
        let builtin_agents = [
            ("forge", include_str!("agents/forge.md")),
            ("muse", include_str!("agents/muse.md")),
            ("sage", include_str!("agents/sage.md")),
        ];

        for (name, content) in builtin_agents {
            let agent = parse_agent_file(content)
                .with_context(|| format!("Failed to parse built-in agent: {name}"))
                .unwrap();

            assert_eq!(agent.id.as_str(), name);
            assert!(agent.title.is_some());
            assert!(agent.description.is_some());
            assert!(agent.system_prompt.is_some());
        }
    }

    #[test]
    fn test_resolve_definition_conflicts_no_duplicates() {
        let fixture = vec![
            AgentDefinition::new("agent1").title("Agent 1"),
            AgentDefinition::new("agent2").title("Agent 2"),
            AgentDefinition::new("agent3").title("Agent 3"),
        ];

        let actual = resolve_definition_conflicts(fixture.clone());

        // Should return all definitions when no conflicts
        assert_eq!(actual.len(), 3);

        let ids: std::collections::HashSet<_> = actual.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains("agent1"));
        assert!(ids.contains("agent2"));
        assert!(ids.contains("agent3"));
    }

    #[test]
    fn test_resolve_definition_conflicts_with_duplicates() {
        let fixture = vec![
            AgentDefinition::new("agent1").title("Global Agent 1"),
            AgentDefinition::new("agent2").title("Global Agent 2"),
            AgentDefinition::new("agent1").title("CWD Agent 1 - Override"), /* Duplicate ID, should override */
            AgentDefinition::new("agent3").title("CWD Agent 3"),
        ];

        let actual = resolve_definition_conflicts(fixture);

        // Should have 3 definitions: agent1 (CWD version), agent2 (global), agent3
        // (CWD)
        assert_eq!(actual.len(), 3);

        let agent1 = actual
            .iter()
            .find(|a| a.id.as_str() == "agent1")
            .expect("Should have agent1");
        let expected_title = "CWD Agent 1 - Override";
        assert_eq!(agent1.title.as_ref().unwrap(), expected_title);
    }

    #[test]
    fn test_resolve_definition_conflicts_multiple_duplicates() {
        // Test scenario: Built-in -> Global -> CWD (CWD should win)
        let fixture = vec![
            AgentDefinition::new("common").title("Built-in Common Agent"),
            AgentDefinition::new("unique1").title("Built-in Unique 1"),
            AgentDefinition::new("common").title("Global Common Agent"), // Override built-in
            AgentDefinition::new("unique2").title("Global Unique 2"),
            AgentDefinition::new("common").title("CWD Common Agent"), // Override global
            AgentDefinition::new("unique3").title("CWD Unique 3"),
        ];

        let actual = resolve_definition_conflicts(fixture);

        // Should have 4 definitions: common (CWD version), unique1, unique2, unique3
        assert_eq!(actual.len(), 4);

        let common = actual
            .iter()
            .find(|a| a.id.as_str() == "common")
            .expect("Should have common agent");
        let expected_title = "CWD Common Agent";
        assert_eq!(common.title.as_ref().unwrap(), expected_title);

        // Verify all unique definitions are present
        let ids: std::collections::HashSet<_> = actual.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains("common"));
        assert!(ids.contains("unique1"));
        assert!(ids.contains("unique2"));
        assert!(ids.contains("unique3"));
    }

    #[test]
    fn test_resolve_definition_conflicts_empty_input() {
        let fixture: Vec<AgentDefinition> = vec![];

        let actual = resolve_definition_conflicts(fixture);

        assert_eq!(actual.len(), 0);
    }
}
