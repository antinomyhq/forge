use std::sync::Arc;

use anyhow::Context;
use forge_app::domain::Skill;
use forge_app::{EnvironmentInfra, FileInfoInfra, FileReaderInfra, Walker, WalkerInfra};
use forge_domain::{PluginRepository, SkillRepository, SkillSource};
use futures::future::join_all;
use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::Deserialize;

/// Repository implementation for loading skills from multiple sources:
/// 1. Built-in skills (embedded in the application)
/// 2. Plugin skills (from each enabled plugin's `skills_paths`)
/// 3. Global custom skills (from ~/forge/skills/ directory)
/// 4. Agents skills (from ~/.agents/skills/ directory)
/// 5. Project-local skills (from .forge/skills/ directory in current working
///    directory)
///
/// ## Skill Precedence
/// When skills have duplicate names across different sources, the precedence
/// order is: **CWD (project-local) > Agents (~/.agents/skills) > Global
/// custom > Plugin > Built-in**
///
/// This means project-local skills can override agents skills, which can
/// override global skills, which can override plugin-provided skills, which
/// can override built-in skills.
///
/// ## Directory Resolution
/// - **Built-in skills**: Embedded in application binary
/// - **Plugin skills**: `<plugin-root>/skills/<skill-name>/SKILL.md`, loaded
///   only for plugins whose `enabled` flag is `true`. Plugin skills are
///   namespaced as `{plugin_name}:{skill_dir_name}` to avoid collisions across
///   plugins.
/// - **Global skills**: `~/forge/skills/<skill-name>/SKILL.md`
/// - **Agents skills**: `~/.agents/skills/<skill-name>/SKILL.md`
/// - **CWD skills**: `./.forge/skills/<skill-name>/SKILL.md` (relative to
///   current working directory)
///
/// Missing directories are handled gracefully and don't prevent loading from
/// other sources.
pub struct ForgeSkillRepository<I> {
    infra: Arc<I>,
    plugin_repository: Option<Arc<dyn PluginRepository>>,
}

impl<I> ForgeSkillRepository<I> {
    /// Construct a skill repository that also loads plugin-provided skills
    /// from the supplied [`PluginRepository`]. This is the production entry
    /// point used by `ForgeRepo::new`.
    pub fn new(infra: Arc<I>, plugin_repository: Arc<dyn PluginRepository>) -> Self {
        Self { infra, plugin_repository: Some(plugin_repository) }
    }

    /// Construct a skill repository with no plugin loader wired in. Only
    /// used by unit tests that do not care about plugin-sourced skills.
    #[cfg(test)]
    pub(crate) fn new_without_plugins(infra: Arc<I>) -> Self {
        Self { infra, plugin_repository: None }
    }

    /// Loads built-in skills that are embedded in the application
    fn load_builtin_skills(&self) -> Vec<Skill> {
        let builtin_skills = vec![
            (
                "forge://skills/create-skill/SKILL.md",
                include_str!("skills/create-skill/SKILL.md"),
            ),
            (
                "forge://skills/execute-plan/SKILL.md",
                include_str!("skills/execute-plan/SKILL.md"),
            ),
            (
                "forge://skills/github-pr-description/SKILL.md",
                include_str!("skills/github-pr-description/SKILL.md"),
            ),
        ];

        builtin_skills
            .into_iter()
            .filter_map(|(path, content)| {
                extract_skill(path, content).map(|s| s.with_source(SkillSource::Builtin))
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl<I: FileInfoInfra + EnvironmentInfra + FileReaderInfra + WalkerInfra> SkillRepository
    for ForgeSkillRepository<I>
{
    /// Loads all available skills from the skills directory
    ///
    /// # Errors
    /// Returns an error if skill loading fails
    async fn load_skills(&self) -> anyhow::Result<Vec<Skill>> {
        let mut skills = Vec::new();
        let env = self.infra.get_environment();

        // Load built-in skills (lowest precedence)
        let builtin_skills = self.load_builtin_skills();
        skills.extend(builtin_skills);

        // Load plugin skills (overrides built-in, below user sources).
        // Plugins are announced through the plugin repository (Phase 1).
        let plugin_skills = self.load_plugin_skills().await;
        skills.extend(plugin_skills);

        // Load global skills
        let global_dir = env.global_skills_path();
        let global_skills = self
            .load_skills_from_dir(&global_dir, SkillSource::GlobalUser)
            .await?;
        skills.extend(global_skills);

        // Load agents skills (~/.agents/skills)
        if let Some(agents_dir) = env.agents_skills_path() {
            let agents_skills = self
                .load_skills_from_dir(&agents_dir, SkillSource::AgentsDir)
                .await?;
            skills.extend(agents_skills);
        }

        // Load project-local skills
        let cwd_dir = env.local_skills_path();
        let cwd_skills = self
            .load_skills_from_dir(&cwd_dir, SkillSource::ProjectCwd)
            .await?;
        skills.extend(cwd_skills);

        // Resolve conflicts by keeping the last occurrence
        // (CWD > AgentsDir > Global > Plugin > Built-in)
        let skills = resolve_skill_conflicts(skills);

        // Render all skills with environment context
        let rendered_skills = skills
            .into_iter()
            .map(|skill| self.render_skill(skill, &env))
            .collect::<Vec<_>>();

        Ok(rendered_skills)
    }
}

impl<I: FileInfoInfra + EnvironmentInfra + FileReaderInfra + WalkerInfra> ForgeSkillRepository<I> {
    /// Loads skills from a specific directory by listing subdirectories first,
    /// then reading SKILL.md from each subdirectory if it exists
    async fn load_skills_from_dir(
        &self,
        dir: &std::path::Path,
        source: SkillSource,
    ) -> anyhow::Result<Vec<Skill>> {
        self.load_skills_from_dir_with_namespace(dir, source, None)
            .await
    }

    /// Loads skills from a plugin skills directory, namespacing each skill's
    /// name as `{plugin_name}:{skill_dir_name}` to avoid collisions across
    /// plugins.
    async fn load_plugin_skills_from_dir(
        &self,
        dir: &std::path::Path,
        plugin_name: &str,
    ) -> anyhow::Result<Vec<Skill>> {
        self.load_skills_from_dir_with_namespace(
            dir,
            SkillSource::Plugin { plugin_name: plugin_name.to_string() },
            Some(plugin_name),
        )
        .await
    }

    /// Walks `dir` one level deep, reads each child `SKILL.md`, and tags the
    /// resulting skills with `source`. When `namespace_plugin` is `Some`,
    /// each loaded skill is renamed to `{plugin_name}:{skill_dir_name}` so
    /// plugin-owned skills cannot collide across plugins.
    async fn load_skills_from_dir_with_namespace(
        &self,
        dir: &std::path::Path,
        source: SkillSource,
        namespace_plugin: Option<&str>,
    ) -> anyhow::Result<Vec<Skill>> {
        if !self.infra.exists(dir).await? {
            return Ok(vec![]);
        }

        let walker = Walker::unlimited()
            .cwd(dir.to_path_buf())
            .max_depth(1_usize)
            .max_breadth(usize::MAX); // Override breadth limit to see all skill directories
        let entries = self
            .infra
            .walk(walker)
            .await
            .with_context(|| format!("Failed to list directory: {}", dir.display()))?;

        // Filter for directories only (entries that end with '/')
        let subdirs: Vec<_> = entries
            .into_iter()
            .filter_map(|walked| {
                if walked.is_dir() && !walked.path.is_empty() {
                    // Construct the full path
                    Some(dir.join(&walked.path))
                } else {
                    None
                }
            })
            .collect();

        // Read SKILL.md from each subdirectory in parallel
        let futures = subdirs.into_iter().map(|subdir| {
            let infra = Arc::clone(&self.infra);
            let source = source.clone();
            let namespace_plugin = namespace_plugin.map(|s| s.to_string());
            async move {
                let skill_path = subdir.join("SKILL.md");

                // Check if SKILL.md exists in this subdirectory
                if infra.exists(&skill_path).await? {
                    // Read the file content
                    match infra.read_utf8(&skill_path).await {
                        Ok(content) => {
                            let path_str = skill_path.display().to_string();
                            let dir_name = subdir
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("unknown")
                                .to_string();

                            // Get all resource files in the skill directory recursively
                            let walker = Walker::unlimited().cwd(subdir.clone());
                            let resources = infra
                                .walk(walker)
                                .await
                                .unwrap_or_default()
                                .into_iter()
                                .filter_map(|walked| {
                                    // Only include files (not directories) and exclude SKILL.md
                                    if !walked.is_dir() {
                                        let full_path = subdir.join(&walked.path);
                                        if full_path.file_name() != skill_path.file_name() {
                                            Some(full_path)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>();

                            // Try to extract skill from front matter, otherwise create with
                            // directory name
                            let mut skill = if let Some(skill) = extract_skill(&path_str, &content)
                            {
                                skill.resources(resources)
                            } else {
                                // Fallback: create skill with directory name if front matter is
                                // missing
                                Skill::new(dir_name.clone(), content, String::new())
                                    .path(path_str)
                                    .resources(resources)
                            };

                            // Namespace plugin skills as `{plugin_name}:{dir_name}` so
                            // multiple plugins cannot collide on the same `SKILL.md`
                            // directory name.
                            if let Some(plugin_name) = namespace_plugin.as_deref() {
                                skill.name = format!("{plugin_name}:{dir_name}");
                            }

                            skill.source = source.clone();
                            Ok(Some(skill))
                        }
                        Err(e) => {
                            // Log warning but continue processing other skills
                            tracing::warn!(
                                "Failed to read skill file {}: {}",
                                skill_path.display(),
                                e
                            );
                            Ok(None)
                        }
                    }
                } else {
                    Ok(None)
                }
            }
        });

        // Execute all futures in parallel and collect results
        let results = join_all(futures).await;
        let skills: Vec<Skill> = results
            .into_iter()
            .filter_map(|result: anyhow::Result<Option<Skill>>| result.ok().flatten())
            .collect();

        Ok(skills)
    }

    /// Loads all plugin-provided skills from every enabled plugin returned
    /// by the injected [`PluginRepository`]. Returns an empty vector when
    /// no plugin repository is wired in (used by unit tests).
    async fn load_plugin_skills(&self) -> Vec<Skill> {
        let Some(plugin_repo) = self.plugin_repository.as_ref() else {
            return Vec::new();
        };

        let plugins = match plugin_repo.load_plugins().await {
            Ok(plugins) => plugins,
            Err(err) => {
                tracing::warn!("Failed to enumerate plugins for skill loading: {err:#}");
                return Vec::new();
            }
        };

        let mut all = Vec::new();
        for plugin in plugins.into_iter().filter(|p| p.enabled) {
            for skills_dir in &plugin.skills_paths {
                match self
                    .load_plugin_skills_from_dir(skills_dir, &plugin.name)
                    .await
                {
                    Ok(loaded) => all.extend(loaded),
                    Err(err) => {
                        tracing::warn!(
                            "Failed to load plugin skills from {}: {err:#}",
                            skills_dir.display()
                        );
                    }
                }
            }
        }

        all
    }

    /// Renders a skill's command field with environment context
    ///
    /// # Arguments
    /// * `skill` - The skill to render
    /// * `env` - The environment containing path informations
    fn render_skill(&self, skill: Skill, env: &forge_domain::Environment) -> Skill {
        let global = env.global_skills_path().display().to_string();
        let agents = env
            .agents_skills_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let local = env.local_skills_path().display().to_string();

        let rendered = skill
            .command
            .replace("{{global_skills_path}}", &global)
            .replace("{{agents_skills_path}}", &agents)
            .replace("{{local_skills_path}}", &local);

        skill.command(rendered)
    }
}

/// Private type for parsing skill YAML front matter
#[derive(Debug, Deserialize)]
struct SkillMetadata {
    /// Optional name of the skill (overrides filename if present)
    name: Option<String>,
    /// Optional description of the skill
    description: Option<String>,
    /// Optional extended guidance describing when the skill should run.
    /// Mirrors Claude Code's `when_to_use` frontmatter field.
    #[serde(default)]
    when_to_use: Option<String>,
    /// Optional allow-list of tool names this skill is permitted to use.
    /// Mirrors Claude Code's `allowed-tools` frontmatter field (hyphenated
    /// on-disk, renamed here so the Rust field stays idiomatic).
    #[serde(rename = "allowed-tools", default)]
    allowed_tools: Option<Vec<String>>,
    /// Matches Claude Code's `disable-model-invocation` frontmatter flag.
    /// When `true` the model cannot auto-invoke the skill; only users can.
    #[serde(rename = "disable-model-invocation", default)]
    disable_model_invocation: bool,
    /// Matches Claude Code's `user-invocable` frontmatter flag. Defaults
    /// to `true` so legacy `SKILL.md` files (which predate the flag)
    /// remain invocable from the CLI.
    #[serde(rename = "user-invocable", default = "default_true")]
    user_invocable: bool,
}

/// Serde helper for the `user_invocable` default. Duplicated from
/// [`forge_domain::skill`] because `#[serde(default = "path")]` only
/// accepts a path that resolves inside the current crate.
fn default_true() -> bool {
    true
}

/// Extracts metadata from the skill markdown content using YAML front matter
///
/// Parses YAML front matter from the markdown content and extracts skill
/// metadata. Expected format:
/// ```markdown
/// ---
/// name: "skill-name"
/// description: "Your description here"
/// when_to_use: "When the user asks to ..."
/// allowed-tools: ["read", "write"]
/// disable-model-invocation: false
/// user-invocable: true
/// ---
/// # Skill content...
/// ```
///
/// The `name` and `description` fields are required. The extended fields
/// (`when_to_use`, `allowed-tools`, `disable-model-invocation`,
/// `user-invocable`) are optional and fall back to documented defaults
/// when absent so pre-existing `SKILL.md` files continue to parse.
fn extract_skill(path: &str, content: &str) -> Option<Skill> {
    let matter = Matter::<YAML>::new();
    let result = matter.parse::<SkillMetadata>(content);
    result.ok().and_then(|parsed| {
        let command = parsed.content;
        let data = parsed.data?;
        let name = data.name?;
        let description = data.description?;

        let mut skill = Skill::new(name, command, description).path(path);
        if let Some(when_to_use) = data.when_to_use {
            skill = skill.when_to_use(when_to_use);
        }
        if let Some(allowed_tools) = data.allowed_tools {
            skill = skill.allowed_tools(allowed_tools);
        }
        skill.disable_model_invocation = data.disable_model_invocation;
        skill.user_invocable = data.user_invocable;
        Some(skill)
    })
}

/// Resolves skill conflicts by keeping the last occurrence of each skill name
///
/// This gives precedence to later sources (CWD > Global)
fn resolve_skill_conflicts(skills: Vec<Skill>) -> Vec<Skill> {
    let mut seen = std::collections::HashMap::new();
    let mut result = Vec::new();

    for skill in skills {
        if let Some(idx) = seen.get(&skill.name) {
            // Replace the earlier skill with the same name
            result[*idx] = skill.clone();
        } else {
            // First occurrence of this skill name
            seen.insert(skill.name.clone(), result.len());
            result.push(skill);
        }
    }

    result
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
    /// loaded plugins. Used to exercise the plugin skill loading path
    /// without touching the filesystem for plugin discovery.
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

    fn fixture_skill_repo() -> (ForgeSkillRepository<ForgeInfra>, std::path::PathBuf) {
        let skill_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fixtures/skills_with_resources");
        let config = ForgeConfig::read().unwrap_or_default();
        let services_url = config.services_url.parse().unwrap();
        let infra = Arc::new(ForgeInfra::new(
            std::env::current_dir().unwrap(),
            config,
            services_url,
        ));
        let repo = ForgeSkillRepository::new_without_plugins(infra);
        (repo, skill_dir)
    }

    fn fixture_plugin(name: &str, enabled: bool, skills_path: PathBuf) -> LoadedPlugin {
        LoadedPlugin {
            name: name.to_string(),
            manifest: PluginManifest { name: Some(name.to_string()), ..Default::default() },
            path: PathBuf::from(format!("/fake/{name}")),
            source: PluginSource::Global,
            enabled,
            is_builtin: false,
            commands_paths: Vec::new(),
            agents_paths: Vec::new(),
            skills_paths: vec![skills_path],
            mcp_servers: None,
        }
    }

    fn fixture_skill_repo_with_plugins(
        plugins: Vec<LoadedPlugin>,
    ) -> ForgeSkillRepository<ForgeInfra> {
        let config = ForgeConfig::read().unwrap_or_default();
        let services_url = config.services_url.parse().unwrap();
        let infra = Arc::new(ForgeInfra::new(
            std::env::current_dir().unwrap(),
            config,
            services_url,
        ));
        let plugin_repo: Arc<dyn PluginRepository> = Arc::new(MockPluginRepository { plugins });
        ForgeSkillRepository::new(infra, plugin_repo)
    }

    #[test]
    fn test_resolve_skill_conflicts() {
        // Fixture
        let skills = vec![
            Skill::new("skill1", "global prompt", "global desc").path("/global/skill1.md"),
            Skill::new("skill2", "prompt2", "desc2").path("/global/skill2.md"),
            Skill::new("skill1", "cwd prompt", "cwd desc").path("/cwd/skill1.md"),
        ];

        // Act
        let actual = resolve_skill_conflicts(skills);

        // Assert
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].name, "skill1");
        assert_eq!(
            actual[0].path,
            Some(std::path::Path::new("/cwd/skill1.md").to_path_buf())
        );
        assert_eq!(actual[0].command, "cwd prompt");
        assert_eq!(actual[1].name, "skill2");
    }

    #[test]
    fn test_resolve_skill_conflicts_user_overrides_plugin_by_name() {
        // A user skill with the same *namespaced* name as a plugin skill
        // should win because it is pushed into the list after the plugin
        // skill by `load_skills`.
        let skills = vec![
            Skill::new("demo:foo", "plugin body", "plugin desc")
                .with_source(SkillSource::Plugin { plugin_name: "demo".into() }),
            Skill::new("demo:foo", "user body", "user desc").with_source(SkillSource::GlobalUser),
        ];

        let actual = resolve_skill_conflicts(skills);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].command, "user body");
        assert_eq!(actual[0].source, SkillSource::GlobalUser);
    }

    #[test]
    fn test_load_builtin_skills() {
        // Fixture
        let repo = ForgeSkillRepository { infra: Arc::new(()), plugin_repository: None };

        // Act
        let actual = repo.load_builtin_skills();

        // Assert
        assert_eq!(actual.len(), 3);

        // Check create-skill
        let create_skill = actual.iter().find(|s| s.name == "create-skill").unwrap();
        assert_eq!(
            create_skill.path,
            Some(std::path::Path::new("forge://skills/create-skill/SKILL.md").to_path_buf())
        );
        assert_eq!(
            create_skill.description,
            "Guide for creating effective skills. This skill should be used when users want to create a new skill (or update an existing skill) that extends your capabilities with specialized knowledge, workflows, or tool integrations."
        );
        assert!(create_skill.command.contains("Skill Creator"));
        assert!(create_skill.command.contains("creating effective skills"));
        assert_eq!(create_skill.source, SkillSource::Builtin);

        // Check execute-plan
        let execute_plan = actual.iter().find(|s| s.name == "execute-plan").unwrap();
        assert_eq!(
            execute_plan.path,
            Some(std::path::Path::new("forge://skills/execute-plan/SKILL.md").to_path_buf())
        );
        assert!(
            execute_plan
                .description
                .contains("Execute structured task plans")
        );
        assert!(execute_plan.command.contains("Execute Plan"));

        // Check github-pr-description
        let pr_description = actual
            .iter()
            .find(|s| s.name == "github-pr-description")
            .unwrap();
        assert_eq!(
            pr_description.path,
            Some(
                std::path::Path::new("forge://skills/github-pr-description/SKILL.md").to_path_buf()
            )
        );
        assert!(!pr_description.description.is_empty());
        assert!(pr_description.command.contains("Create PR Description"));
    }

    #[tokio::test]
    async fn test_extract_skill_with_valid_metadata() {
        // Fixture
        let path = "fixtures/skills/with_name_and_description.md";
        let content =
            forge_test_kit::fixture!("/src/fixtures/skills/with_name_and_description.md").await;

        // Act
        let actual = extract_skill(path, &content);

        // Assert
        let expected = Some(
            Skill::new(
                "pdf-handler",
                "# PDF Handler\n\nContent here...",
                "This is a skill for handling PDF files",
            )
            .path(path),
        );
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_extract_skill_with_incomplete_metadata() {
        // Fixture
        let content = forge_test_kit::fixture!("/src/fixtures/skills/with_name_only.md").await;

        // Act
        let actual = extract_skill("test.md", &content);

        // Assert - Returns None because metadata is incomplete
        assert_eq!(actual, None);
    }

    #[tokio::test]
    async fn test_extract_skill_defaults_extended_fields_when_absent() {
        // A skill whose frontmatter only specifies the required
        // `name`/`description` fields must still parse and must receive
        // the documented defaults for the extended Claude Code fields.
        let path = "fixtures/skills/with_name_and_description.md";
        let content =
            forge_test_kit::fixture!("/src/fixtures/skills/with_name_and_description.md").await;

        let actual = extract_skill(path, &content).expect("skill should parse");

        assert_eq!(actual.when_to_use, None);
        assert_eq!(actual.allowed_tools, None);
        assert!(!actual.disable_model_invocation);
        assert!(actual.user_invocable);
    }

    #[tokio::test]
    async fn test_extract_skill_parses_all_extended_fields() {
        // A skill whose frontmatter populates every Claude Code field
        // should surface those values on the resulting `Skill`. The
        // hyphenated on-disk field names (`allowed-tools`,
        // `disable-model-invocation`, `user-invocable`) must map onto
        // the idiomatic Rust field names.
        let content = r#"---
name: "full-skill"
description: "Skill with all extended frontmatter fields populated"
when_to_use: "Invoke when the user asks for deep analysis"
allowed-tools:
  - "read"
  - "write"
  - "shell"
disable-model-invocation: true
user-invocable: false
---

# Full Skill

Body content for the extended frontmatter test.
"#;

        let actual = extract_skill("inline", content).expect("skill should parse");

        assert_eq!(actual.name, "full-skill");
        assert_eq!(
            actual.description,
            "Skill with all extended frontmatter fields populated"
        );
        assert_eq!(
            actual.when_to_use.as_deref(),
            Some("Invoke when the user asks for deep analysis")
        );
        assert_eq!(
            actual.allowed_tools.as_deref(),
            Some(["read".to_string(), "write".to_string(), "shell".to_string()].as_slice())
        );
        assert!(actual.disable_model_invocation);
        assert!(!actual.user_invocable);
    }

    #[tokio::test]
    async fn test_extract_skill_disable_model_invocation_flag() {
        // Explicitly set `disable-model-invocation: true` without any
        // other extended fields and confirm only that flag flips while
        // `user_invocable` stays at its default `true`.
        let content = r#"---
name: "restricted"
description: "Only users may invoke this skill"
disable-model-invocation: true
---

Body.
"#;

        let actual = extract_skill("inline", content).expect("skill should parse");

        assert!(actual.disable_model_invocation);
        assert!(actual.user_invocable);
        assert_eq!(actual.when_to_use, None);
        assert_eq!(actual.allowed_tools, None);
    }

    #[tokio::test]
    async fn test_extract_skill_user_invocable_false_flag() {
        // Explicitly set `user-invocable: false` and confirm the flag
        // flips while `disable_model_invocation` stays at its default
        // `false`.
        let content = r#"---
name: "model-only"
description: "Only the model may invoke this skill"
user-invocable: false
---

Body.
"#;

        let actual = extract_skill("inline", content).expect("skill should parse");

        assert!(!actual.disable_model_invocation);
        assert!(!actual.user_invocable);
    }

    #[tokio::test]
    async fn test_load_skills_from_dir() {
        // Fixture
        let (repo, skill_dir) = fixture_skill_repo();

        // Act
        let actual = repo
            .load_skills_from_dir(&skill_dir, SkillSource::GlobalUser)
            .await
            .unwrap();

        // Assert - should load all skills
        assert_eq!(actual.len(), 2); // minimal-skill, test-skill

        // Verify skill with no resources
        let minimal_skill = actual.iter().find(|s| s.name == "minimal-skill").unwrap();
        assert_eq!(minimal_skill.resources.len(), 0);
        assert_eq!(minimal_skill.source, SkillSource::GlobalUser);

        // Verify skill with nested resources
        let test_skill = actual.iter().find(|s| s.name == "test-skill").unwrap();
        assert_eq!(test_skill.description, "A test skill with resources");
        assert_eq!(test_skill.resources.len(), 3); // file_1.txt, foo/file_2.txt, foo/bar/file_3.txt
        assert_eq!(test_skill.source, SkillSource::GlobalUser);

        // Verify nested directory structure is captured
        assert!(
            test_skill
                .resources
                .iter()
                .any(|p| p.ends_with("file_1.txt"))
        );
        assert!(
            test_skill
                .resources
                .iter()
                .any(|p| p.ends_with("foo/file_2.txt"))
        );
        assert!(
            test_skill
                .resources
                .iter()
                .any(|p| p.ends_with("foo/bar/file_3.txt"))
        );

        // Ensure SKILL.md is never included in resources
        assert!(actual.iter().all(|s| {
            !s.resources
                .iter()
                .any(|p| p.file_name().unwrap() == "SKILL.md")
        }));
    }

    #[tokio::test]
    async fn test_load_plugin_skills_namespaces_and_tags_source() {
        let skill_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fixtures/skills_with_resources");
        let plugin = fixture_plugin("demo", true, skill_dir);
        let repo = fixture_skill_repo_with_plugins(vec![plugin]);

        let actual = repo.load_plugin_skills().await;

        // Two skills in the fixture directory, both should be loaded.
        assert_eq!(actual.len(), 2);

        // Every loaded skill must be namespaced with the plugin name
        // and tagged with SkillSource::Plugin.
        for skill in &actual {
            assert!(
                skill.name.starts_with("demo:"),
                "expected namespaced name, got {}",
                skill.name
            );
            assert_eq!(
                skill.source,
                SkillSource::Plugin { plugin_name: "demo".to_string() }
            );
        }

        // Specific expected names.
        assert!(actual.iter().any(|s| s.name == "demo:minimal-skill"));
        assert!(actual.iter().any(|s| s.name == "demo:test-skill"));
    }

    #[tokio::test]
    async fn test_load_plugin_skills_skips_disabled_plugins() {
        let skill_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fixtures/skills_with_resources");
        let plugin = fixture_plugin("demo", false, skill_dir);
        let repo = fixture_skill_repo_with_plugins(vec![plugin]);

        let actual = repo.load_plugin_skills().await;
        assert!(
            actual.is_empty(),
            "disabled plugin skills should be skipped"
        );
    }

    #[tokio::test]
    async fn test_load_plugin_skills_handles_missing_skills_dir() {
        let missing = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fixtures/definitely-does-not-exist");
        let plugin = fixture_plugin("demo", true, missing);
        let repo = fixture_skill_repo_with_plugins(vec![plugin]);

        let actual = repo.load_plugin_skills().await;
        assert!(actual.is_empty());
    }
}
