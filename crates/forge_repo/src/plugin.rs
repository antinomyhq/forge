use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;
use forge_app::domain::{
    LoadedPlugin, McpServerConfig, PluginComponentPath, PluginLoadError, PluginLoadErrorKind,
    PluginLoadResult, PluginManifest, PluginRepository, PluginSource,
};
use forge_app::{DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra, FileReaderInfra};
use forge_config::PluginSetting;
use futures::future::join_all;

/// Forge implementation of [`PluginRepository`].
///
/// Discovers plugins by scanning two directories:
///
/// 1. **Global**: `~/forge/plugins/<plugin>/` (from `Environment::plugin_path`)
/// 2. **Project-local**: `./.forge/plugins/<plugin>/` (from
///    `Environment::plugin_cwd_path`)
///
/// For each subdirectory the loader probes for a manifest file in priority
/// order:
///
/// 1. `<plugin>/.forge-plugin/plugin.json` (Forge-native marker)
/// 2. `<plugin>/.claude-plugin/plugin.json` (Claude Code 1:1 compatibility)
/// 3. `<plugin>/plugin.json` (legacy/bare)
///
/// When more than one marker is present the loader prefers the Forge-native
/// one and emits a `tracing::warn` to flag the ambiguity.
///
/// ## Precedence
///
/// When the same plugin name appears in both directories, the project-local
/// copy wins. This mirrors `claude-code/src/utils/plugins/pluginLoader.ts`
/// which gives workspace-scoped plugins precedence over global ones.
///
/// ## Component path resolution
///
/// Manifest fields `commands`, `agents` and `skills` are optional. If a
/// manifest omits them, the loader auto-detects sibling directories named
/// `commands/`, `agents/` and `skills/` at the plugin root. Manifest values
/// always take precedence over auto-detection — even when they point to a
/// non-existent path (so the user notices the typo).
///
/// ## MCP servers
///
/// MCP server definitions can come from either `manifest.mcp_servers`
/// (inline) or a sibling `.mcp.json` file at the plugin root. When both
/// are present they are merged with the inline manifest entries winning.
///
/// ## Error handling
///
/// Per-plugin failures (malformed JSON, missing required fields, unreadable
/// `hooks.json`) are logged via `tracing::warn` and the plugin is skipped.
/// Top-level filesystem errors (e.g. permission denied on the parent
/// directory) bubble up. Discovery never fails the whole CLI startup just
/// because one plugin is broken.
pub struct ForgePluginRepository<I> {
    infra: Arc<I>,
}

impl<I> ForgePluginRepository<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<I> PluginRepository for ForgePluginRepository<I>
where
    I: EnvironmentInfra<Config = forge_config::ForgeConfig>
        + FileReaderInfra
        + FileInfoInfra
        + DirectoryReaderInfra,
{
    async fn load_plugins(&self) -> anyhow::Result<Vec<LoadedPlugin>> {
        // Delegate to the error-surfacing variant and discard the diagnostic
        // tail so existing call sites keep their old signature.
        self.load_plugins_with_errors().await.map(|r| r.plugins)
    }

    async fn load_plugins_with_errors(&self) -> anyhow::Result<PluginLoadResult> {
        let env = self.infra.get_environment();
        let config = self.infra.get_config().ok();
        let plugin_settings: BTreeMap<String, PluginSetting> =
            config.and_then(|cfg| cfg.plugins).unwrap_or_default();

        // Collect all scan roots. Order matters for `resolve_plugin_conflicts`
        // which uses last-wins semantics:
        //   Claude Code global < Forge global < Claude Code project < Forge project
        let mut scan_futures: Vec<_> = Vec::new();

        // 1. Claude Code global (~/.claude/plugins/) — lowest precedence.
        if let Some(claude_global) = env.claude_plugin_path() {
            scan_futures.push(self.scan_root_owned(claude_global, PluginSource::ClaudeCode));
        }

        // 2. Forge global (~/forge/plugins/).
        scan_futures.push(self.scan_root_owned(env.plugin_path(), PluginSource::Global));

        // 3. Claude Code project-local (.claude/plugins/).
        scan_futures
            .push(self.scan_root_owned(env.claude_plugin_cwd_path(), PluginSource::ClaudeCode));

        // 4. Forge project-local (.forge/plugins/) — highest precedence.
        scan_futures.push(self.scan_root_owned(env.plugin_cwd_path(), PluginSource::Project));

        let results = join_all(scan_futures).await;

        let (mut plugins, mut errors): (Vec<LoadedPlugin>, Vec<PluginLoadError>) =
            (Vec::new(), Vec::new());

        for result in results {
            let (p, e) = result?;
            plugins.extend(p);
            errors.extend(e);
        }

        // Apply last-wins precedence: Forge project > Claude project >
        // Forge global > Claude global.
        let plugins = resolve_plugin_conflicts(plugins);

        // Apply enabled overrides from .forge.toml.
        let plugins = plugins
            .into_iter()
            .map(|mut plugin| {
                if let Some(setting) = plugin_settings.get(&plugin.name) {
                    plugin.enabled = setting.enabled;
                }
                plugin
            })
            .collect();

        Ok(PluginLoadResult { plugins, errors })
    }
}

impl<I> ForgePluginRepository<I>
where
    I: FileReaderInfra + FileInfoInfra + DirectoryReaderInfra,
{
    /// Owned-path convenience wrapper around [`scan_root`] for use with
    /// `join_all` where futures must be `'static`.
    async fn scan_root_owned(
        &self,
        root: PathBuf,
        source: PluginSource,
    ) -> anyhow::Result<(Vec<LoadedPlugin>, Vec<PluginLoadError>)> {
        self.scan_root(&root, source).await
    }

    /// Scans a single root directory and returns all plugins discovered
    /// underneath along with any per-plugin load errors.
    ///
    /// Subdirectories without a recognised manifest file are silently
    /// skipped. Malformed manifests (unreadable, bad JSON, missing fields)
    /// are logged via `tracing::warn` for immediate operator visibility
    /// and also accumulated into the returned error vector so the Phase 9
    /// `:plugin list` command can surface them to the user.
    async fn scan_root(
        &self,
        root: &Path,
        source: PluginSource,
    ) -> anyhow::Result<(Vec<LoadedPlugin>, Vec<PluginLoadError>)> {
        if !self.infra.exists(root).await? {
            return Ok((Vec::new(), Vec::new()));
        }

        let entries = self
            .infra
            .list_directory_entries(root)
            .await
            .with_context(|| format!("Failed to list plugin root: {}", root.display()))?;

        let load_futs = entries
            .into_iter()
            .filter(|(_, is_dir)| *is_dir)
            .map(|(path, _)| {
                let infra = Arc::clone(&self.infra);
                let source_copy = source;
                async move {
                    let result = load_one_plugin(infra, path.clone(), source_copy).await;
                    (path, result)
                }
            });

        let results = join_all(load_futs).await;
        let mut plugins = Vec::new();
        let mut errors = Vec::new();
        for (path, res) in results {
            match res {
                Ok(Some(plugin)) => plugins.push(plugin),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Failed to load plugin: {e:#}");
                    // Capture the directory name (if any) as a best-effort
                    // plugin identifier; callers render this alongside the
                    // error message in `:plugin list`.
                    let plugin_name = path.file_name().and_then(|s| s.to_str()).map(String::from);
                    errors.push(PluginLoadError {
                        plugin_name,
                        path,
                        kind: PluginLoadErrorKind::Other,
                        error: format!("{e:#}"),
                    });
                }
            }
        }

        Ok((plugins, errors))
    }
}

/// Loads a single plugin directory.
///
/// Returns:
/// - `Ok(Some(plugin))` when a manifest was found and parsed successfully
/// - `Ok(None)` when no manifest is present (the directory is not a plugin)
/// - `Err(_)` when a manifest was found but could not be parsed
async fn load_one_plugin<I>(
    infra: Arc<I>,
    plugin_dir: PathBuf,
    source: PluginSource,
) -> anyhow::Result<Option<LoadedPlugin>>
where
    I: FileReaderInfra + FileInfoInfra + DirectoryReaderInfra,
{
    let manifest_path = match find_manifest(&infra, &plugin_dir).await? {
        Some(path) => path,
        None => return Ok(None),
    };

    let raw = infra
        .read_utf8(&manifest_path)
        .await
        .with_context(|| format!("Failed to read manifest: {}", manifest_path.display()))?;

    let manifest: PluginManifest = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse manifest: {}", manifest_path.display()))?;

    let dir_name = plugin_dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(String::from)
        .unwrap_or_else(|| "<unknown>".to_string());

    let name = manifest.name.clone().unwrap_or_else(|| dir_name.clone());

    // Resolve component paths.
    let commands_paths =
        resolve_component_dirs(&infra, &plugin_dir, manifest.commands.as_ref(), "commands").await;
    let agents_paths =
        resolve_component_dirs(&infra, &plugin_dir, manifest.agents.as_ref(), "agents").await;
    let skills_paths =
        resolve_component_dirs(&infra, &plugin_dir, manifest.skills.as_ref(), "skills").await;

    // Resolve MCP servers: merge inline manifest entries with sibling .mcp.json
    // when present.
    let mcp_servers = resolve_mcp_servers(&infra, &plugin_dir, &manifest).await;

    Ok(Some(LoadedPlugin {
        name,
        manifest,
        path: plugin_dir,
        source,
        // Plugins are enabled by default; the caller will apply ForgeConfig
        // overrides afterwards.
        enabled: true,
        is_builtin: false,
        commands_paths,
        agents_paths,
        skills_paths,
        mcp_servers,
    }))
}

/// Locates the manifest file inside a plugin directory.
///
/// Probes in priority order:
/// 1. `.forge-plugin/plugin.json`
/// 2. `.claude-plugin/plugin.json`
/// 3. `plugin.json`
///
/// When more than one marker is present, the function returns the
/// highest-priority match and logs a warning so the user is aware of the
/// ambiguity.
async fn find_manifest<I>(infra: &Arc<I>, plugin_dir: &Path) -> anyhow::Result<Option<PathBuf>>
where
    I: FileInfoInfra,
{
    let candidates = [
        plugin_dir.join(".forge-plugin").join("plugin.json"),
        plugin_dir.join(".claude-plugin").join("plugin.json"),
        plugin_dir.join("plugin.json"),
    ];

    let results = futures::future::join_all(candidates.iter().map(|p| infra.exists(p))).await;

    let mut found = Vec::new();
    for (path, result) in candidates.iter().zip(results) {
        if result? {
            found.push(path.clone());
        }
    }

    if found.len() > 1 {
        tracing::warn!(
            "Plugin {} has multiple manifest files; using {} (other candidates: {:?})",
            plugin_dir.display(),
            found[0].display(),
            &found[1..]
        );
    }

    Ok(found.into_iter().next())
}

/// Resolves a component directory list (`commands`, `agents`, `skills`).
///
/// When the manifest declared explicit paths, those win even if they point
/// to non-existent directories — the user gets a chance to see the typo via
/// follow-up validation. When the manifest is silent, the auto-detected
/// `<plugin>/<default_name>/` is returned only if it exists on disk.
async fn resolve_component_dirs<I>(
    infra: &Arc<I>,
    plugin_dir: &Path,
    declared: Option<&PluginComponentPath>,
    default_name: &str,
) -> Vec<PathBuf>
where
    I: FileInfoInfra,
{
    if let Some(spec) = declared {
        return spec
            .as_paths()
            .into_iter()
            .map(|p| plugin_dir.join(p))
            .collect();
    }

    let auto = plugin_dir.join(default_name);
    match infra.exists(&auto).await {
        Ok(true) => vec![auto],
        _ => Vec::new(),
    }
}

/// Resolves MCP server definitions for a plugin.
///
/// Inline manifest entries always win over `.mcp.json`. The merge is shallow:
/// for each server name only one definition is kept.
async fn resolve_mcp_servers<I>(
    infra: &Arc<I>,
    plugin_dir: &Path,
    manifest: &PluginManifest,
) -> Option<BTreeMap<String, McpServerConfig>>
where
    I: FileReaderInfra + FileInfoInfra,
{
    let mut merged: BTreeMap<String, McpServerConfig> = BTreeMap::new();

    // Sibling .mcp.json contributes first.
    let sidecar = plugin_dir.join(".mcp.json");
    if matches!(infra.exists(&sidecar).await, Ok(true))
        && let Ok(raw) = infra.read_utf8(&sidecar).await
    {
        // .mcp.json typically wraps servers under "mcpServers". Try that
        // shape first; fall back to a bare map for compat with simpler
        // hand-written files.
        #[derive(serde::Deserialize)]
        struct McpJsonFile {
            #[serde(default, alias = "mcpServers")]
            mcp_servers: BTreeMap<String, McpServerConfig>,
        }

        if let Ok(parsed) = serde_json::from_str::<McpJsonFile>(&raw) {
            merged.extend(parsed.mcp_servers);
        } else if let Ok(bare) = serde_json::from_str::<BTreeMap<String, McpServerConfig>>(&raw) {
            merged.extend(bare);
        } else {
            tracing::warn!(
                "Plugin .mcp.json {} is not valid: ignored",
                sidecar.display()
            );
        }
    }

    // Inline manifest entries override sidecar entries with the same key.
    if let Some(inline) = &manifest.mcp_servers {
        for (name, cfg) in inline {
            merged.insert(name.clone(), cfg.clone());
        }
    }

    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

/// Resolves duplicate plugin names by keeping the *last* occurrence.
///
/// Because [`ForgePluginRepository::load_plugins`] pushes global plugins
/// before project-local ones, "last wins" implements the documented
/// `Project > Global` precedence.
fn resolve_plugin_conflicts(plugins: Vec<LoadedPlugin>) -> Vec<LoadedPlugin> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut result: Vec<LoadedPlugin> = Vec::new();

    for plugin in plugins {
        if let Some(idx) = seen.get(&plugin.name) {
            result[*idx] = plugin;
        } else {
            seen.insert(plugin.name.clone(), result.len());
            result.push(plugin);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use std::fs;

    use forge_app::domain::PluginSource;
    use forge_config::ForgeConfig;
    use forge_infra::ForgeInfra;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    fn fixture_plugin(name: &str, source: PluginSource) -> LoadedPlugin {
        LoadedPlugin {
            name: name.to_string(),
            manifest: PluginManifest { name: Some(name.to_string()), ..Default::default() },
            path: PathBuf::from("/fake").join(name),
            source,
            enabled: true,
            is_builtin: false,
            commands_paths: Vec::new(),
            agents_paths: Vec::new(),
            skills_paths: Vec::new(),
            mcp_servers: None,
        }
    }

    /// Builds a real [`ForgePluginRepository`] backed by [`ForgeInfra`].
    ///
    /// Mirrors the `fixture_skill_repo` helper in `src/skill.rs`: we use the
    /// production infra (real filesystem I/O) because `scan_root` probes
    /// nested directories, checks manifest markers and reads JSON, and
    /// replicating that semantics in a fake is tedious and error-prone.
    fn fixture_plugin_repo() -> ForgePluginRepository<ForgeInfra> {
        let config = ForgeConfig::read().unwrap_or_default();
        let services_url = config.services_url.parse().unwrap();
        let infra = Arc::new(ForgeInfra::new(
            std::env::current_dir().unwrap(),
            config,
            services_url,
        ));
        ForgePluginRepository::new(infra)
    }

    #[test]
    fn test_resolve_plugin_conflicts_keeps_last() {
        let plugins = vec![
            fixture_plugin("alpha", PluginSource::Global),
            fixture_plugin("beta", PluginSource::Global),
            fixture_plugin("alpha", PluginSource::Project),
        ];

        let actual = resolve_plugin_conflicts(plugins);

        assert_eq!(actual.len(), 2);
        let alpha = actual.iter().find(|p| p.name == "alpha").unwrap();
        assert_eq!(alpha.source, PluginSource::Project);
        let beta = actual.iter().find(|p| p.name == "beta").unwrap();
        assert_eq!(beta.source, PluginSource::Global);
    }

    #[test]
    fn test_resolve_plugin_conflicts_no_duplicates() {
        let plugins = vec![
            fixture_plugin("alpha", PluginSource::Global),
            fixture_plugin("beta", PluginSource::Project),
        ];

        let actual = resolve_plugin_conflicts(plugins);

        assert_eq!(actual.len(), 2);
    }

    /// Verifies four-way precedence: ClaudeCode < Global < ClaudeCode
    /// project < Project (Forge project).
    ///
    /// Simulates the extend order used by `load_plugins_with_errors`:
    /// Claude Code global first, Forge global second, Claude Code
    /// project third, Forge project last. `resolve_plugin_conflicts`
    /// keeps the last occurrence, so Forge project wins.
    #[test]
    fn test_resolve_plugin_conflicts_four_way_precedence() {
        let plugins = vec![
            fixture_plugin("alpha", PluginSource::ClaudeCode), // Claude global
            fixture_plugin("alpha", PluginSource::Global),     // Forge global
            fixture_plugin("alpha", PluginSource::ClaudeCode), // Claude project
            fixture_plugin("alpha", PluginSource::Project),    // Forge project
        ];

        let actual = resolve_plugin_conflicts(plugins);

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].source, PluginSource::Project);
    }

    /// Forge global wins over Claude Code global when there is no project
    /// override.
    #[test]
    fn test_resolve_plugin_conflicts_forge_global_beats_claude_global() {
        let plugins = vec![
            fixture_plugin("alpha", PluginSource::ClaudeCode), // Claude global
            fixture_plugin("alpha", PluginSource::Global),     // Forge global
        ];

        let actual = resolve_plugin_conflicts(plugins);

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].source, PluginSource::Global);
    }

    /// Claude Code project-scoped plugin shadows Claude Code global
    /// plugin with same name (same source, different scope).
    #[tokio::test]
    async fn test_discover_claude_project_shadows_claude_global() {
        let temp = TempDir::new().unwrap();
        let claude_global_root = temp.path().join("claude-global");
        let claude_project_root = temp.path().join("claude-project");
        fs::create_dir_all(&claude_global_root).unwrap();
        fs::create_dir_all(&claude_project_root).unwrap();

        let src = wave_g1_fixtures_root().join("bash-logger");
        copy_dir_recursive(&src, &claude_global_root.join("bash-logger")).unwrap();
        copy_dir_recursive(&src, &claude_project_root.join("bash-logger")).unwrap();

        let repo = fixture_plugin_repo();

        // Mimic load order: Claude global first, then Claude project.
        let (mut combined, mut all_errors): (Vec<LoadedPlugin>, Vec<PluginLoadError>) =
            (Vec::new(), Vec::new());

        let (g, ge) = repo
            .scan_root(&claude_global_root, PluginSource::ClaudeCode)
            .await
            .unwrap();
        combined.extend(g);
        all_errors.extend(ge);

        let (p, pe) = repo
            .scan_root(&claude_project_root, PluginSource::ClaudeCode)
            .await
            .unwrap();
        combined.extend(p);
        all_errors.extend(pe);

        assert!(all_errors.is_empty());

        let resolved = resolve_plugin_conflicts(combined);

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "bash-logger");
        assert!(
            resolved[0].path.starts_with(&claude_project_root),
            "Claude project copy must win over Claude global copy"
        );
    }

    /// Integration-style test exercising the full Claude Code
    /// (`.claude-plugin/plugin.json`) discovery path against a fixture
    /// directory checked in under `src/fixtures/plugins/`.
    ///
    /// Verifies that:
    /// - the `.claude-plugin/plugin.json` marker (not the Forge-native
    ///   `.forge-plugin/plugin.json`) is detected,
    /// - `manifest` fields (name, version, description, author, hooks) are
    ///   parsed correctly,
    /// - declared component paths (commands, skills, agents) resolve to
    ///   absolute paths rooted at the plugin directory, and
    /// - `PluginSource` reflects the value supplied by the caller.
    #[tokio::test]
    async fn test_scan_root_loads_claude_code_plugin_fixture() {
        // Fixture: a real on-disk Claude Code-style plugin layout.
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/plugins");

        let repo = fixture_plugin_repo();
        let (plugins, errors) = repo
            .scan_root(&fixture_root, PluginSource::Project)
            .await
            .expect("scan_root should succeed for a healthy fixture");

        assert!(
            errors.is_empty(),
            "Claude Code fixture must load cleanly, but got errors: {errors:?}"
        );
        assert_eq!(
            plugins.len(),
            1,
            "Expected exactly one plugin under the fixture root"
        );

        let plugin = &plugins[0];
        assert_eq!(plugin.name, "claude-code-demo");
        assert_eq!(plugin.manifest.version.as_deref(), Some("0.1.0"));
        assert_eq!(
            plugin.manifest.description.as_deref(),
            Some(
                "Claude Code 1:1 compatibility fixture used to verify ForgePluginRepository discovery."
            )
        );
        assert_eq!(plugin.source, PluginSource::Project);
        assert!(
            plugin.enabled,
            "plugins default to enabled before config overrides"
        );
        assert!(!plugin.is_builtin);

        // Author should come through the detailed form.
        match &plugin.manifest.author {
            Some(forge_domain::PluginAuthor::Detailed { name, email, url }) => {
                assert_eq!(name, "Forge Test Harness");
                assert_eq!(email.as_deref(), Some("test@forgecode.dev"));
                assert!(url.is_none());
            }
            other => panic!("expected detailed author, got {other:?}"),
        }

        // Component paths must be resolved relative to the plugin root.
        let expected_root = fixture_root.join("claude_code_plugin");
        assert_eq!(plugin.path, expected_root);

        assert_eq!(plugin.commands_paths.len(), 1);
        assert!(
            plugin.commands_paths[0].ends_with("claude_code_plugin/commands"),
            "commands path should resolve to <plugin>/commands, got {:?}",
            plugin.commands_paths[0]
        );

        assert_eq!(plugin.skills_paths.len(), 1);
        assert!(
            plugin.skills_paths[0].ends_with("claude_code_plugin/skills"),
            "skills path should resolve to <plugin>/skills, got {:?}",
            plugin.skills_paths[0]
        );

        assert_eq!(plugin.agents_paths.len(), 1);
        assert!(
            plugin.agents_paths[0].ends_with("claude_code_plugin/agents"),
            "agents path should resolve to <plugin>/agents, got {:?}",
            plugin.agents_paths[0]
        );

        // No MCP servers were declared.
        assert!(plugin.mcp_servers.is_none());
    }

    // =========================================================================
    // Wave G-1 — Phase 11.1.2 plugin discovery integration tests.
    //
    // These tests exercise `ForgePluginRepository::scan_root` against the
    // Wave G-1 fixture plugin catalog checked in under
    // `crates/forge_services/tests/fixtures/plugins/`. The fixtures live in
    // `forge_services` (per the Phase 11.1.1 plan) because downstream
    // Wave G-2+ hook execution tests consume them from inside that crate.
    // The discovery tests must live here in `forge_repo` because
    // `ForgePluginRepository` is private to this crate (`mod plugin;` in
    // `lib.rs` is not `pub`).
    //
    // The tests reference the shared fixtures via the cross-crate relative
    // path `../forge_services/tests/fixtures/plugins` rooted at
    // `forge_repo`'s `CARGO_MANIFEST_DIR`.
    // =========================================================================

    /// Absolute path to the Wave G-1 fixture plugin catalog.
    ///
    /// The catalog lives in `forge_services` (see
    /// `crates/forge_services/tests/fixtures/plugins/`) so hook-execution
    /// tests in Wave G-2 can locate it from inside that crate. This helper
    /// crosses the crate boundary via a `CARGO_MANIFEST_DIR`-rooted
    /// relative path so tests remain hermetic (no cwd dependency).
    fn wave_g1_fixtures_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("forge_services")
            .join("tests")
            .join("fixtures")
            .join("plugins")
    }

    /// Full list of Wave G-1 fixture plugin names, kept in sync with
    /// `crates/forge_services/tests/common/mod.rs::FIXTURE_PLUGIN_NAMES`.
    const WAVE_G1_FIXTURE_NAMES: &[&str] = &[
        "agent-provider",
        "bash-logger",
        "command-provider",
        "config-watcher",
        "dangerous-guard",
        "full-stack",
        "prettier-format",
        "skill-provider",
    ];

    /// Recursively copies a directory tree. Used to stage fixture plugins
    /// into isolated temp directories for the shadow-precedence test. We
    /// deliberately avoid pulling in a new dependency (e.g. `fs_extra`)
    /// and keep the helper local to this test module.
    fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let src = entry.path();
            let dst = to.join(entry.file_name());
            let ft = entry.file_type()?;
            if ft.is_dir() {
                copy_dir_recursive(&src, &dst)?;
            } else if ft.is_file() {
                fs::copy(&src, &dst)?;
            }
        }
        Ok(())
    }

    /// Wave G-1 Phase 11.1.2 test 1: discovery finds every fixture plugin.
    ///
    /// Points `scan_root` at the Wave G-1 fixture catalog and asserts that
    /// all 8 plugins are loaded cleanly with no error tail.
    #[tokio::test]
    async fn test_discover_finds_all_fixture_plugins() {
        let fixture_root = wave_g1_fixtures_root();
        assert!(
            fixture_root.is_dir(),
            "Wave G-1 fixtures must exist at {:?}",
            fixture_root
        );

        let repo = fixture_plugin_repo();
        let (plugins, errors) = repo
            .scan_root(&fixture_root, PluginSource::Project)
            .await
            .expect("scan_root should succeed for the Wave G-1 fixture catalog");

        assert!(
            errors.is_empty(),
            "Wave G-1 fixtures must load cleanly, got errors: {errors:?}"
        );
        assert_eq!(
            plugins.len(),
            WAVE_G1_FIXTURE_NAMES.len(),
            "expected exactly {} plugins, got {}: {:?}",
            WAVE_G1_FIXTURE_NAMES.len(),
            plugins.len(),
            plugins.iter().map(|p| &p.name).collect::<Vec<_>>()
        );

        let mut actual_names: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
        actual_names.sort();
        let mut expected: Vec<&str> = WAVE_G1_FIXTURE_NAMES.to_vec();
        expected.sort();
        assert_eq!(actual_names, expected);

        // Per-plugin spot checks that the most important semantic fields
        // made it through the manifest parser.
        let by_name: std::collections::HashMap<&str, &LoadedPlugin> =
            plugins.iter().map(|p| (p.name.as_str(), p)).collect();

        // skill-provider must resolve a skills/ sibling directory.
        let sp = by_name["skill-provider"];
        assert_eq!(sp.skills_paths.len(), 1);
        assert!(sp.skills_paths[0].ends_with("skill-provider/skills"));

        // command-provider must resolve a commands/ sibling directory.
        let cp = by_name["command-provider"];
        assert_eq!(cp.commands_paths.len(), 1);
        assert!(cp.commands_paths[0].ends_with("command-provider/commands"));

        // agent-provider must resolve an agents/ sibling directory.
        let ap = by_name["agent-provider"];
        assert_eq!(ap.agents_paths.len(), 1);
        assert!(ap.agents_paths[0].ends_with("agent-provider/agents"));

        // full-stack exercises every component type + MCP sidecar.
        let fs_plugin = by_name["full-stack"];
        assert_eq!(fs_plugin.skills_paths.len(), 1);
        assert_eq!(fs_plugin.commands_paths.len(), 1);
        assert_eq!(fs_plugin.agents_paths.len(), 1);
        let mcp = fs_plugin
            .mcp_servers
            .as_ref()
            .expect("full-stack must load its .mcp.json sidecar");
        assert!(
            mcp.contains_key("full-stack-server"),
            "full-stack mcpServers must contain full-stack-server, got {:?}",
            mcp.keys().collect::<Vec<_>>()
        );

        // All plugins are enabled by default (before any config overrides).
        for p in &plugins {
            assert!(p.enabled, "{} must be enabled by default", p.name);
            assert!(!p.is_builtin, "{} must not be flagged as builtin", p.name);
            assert_eq!(p.source, PluginSource::Project);
        }
    }

    /// Wave G-1 Phase 11.1.2 test 2: discovery skips invalid manifests
    /// without crashing and surfaces them in the error tail.
    ///
    /// Stages a tempdir with two plugin directories:
    /// - `valid-plugin` — a copy of the `bash-logger` Wave G-1 fixture
    /// - `broken-plugin` — a directory whose `.claude-plugin/plugin.json` is
    ///   malformed JSON
    ///
    /// `scan_root` must return the valid one in `plugins` and the broken
    /// one in `errors`, without bubbling up a top-level failure.
    #[tokio::test]
    async fn test_discover_skips_invalid_manifest() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Copy the bash-logger fixture as the "valid" plugin.
        let src = wave_g1_fixtures_root().join("bash-logger");
        copy_dir_recursive(&src, &root.join("valid-plugin")).unwrap();
        // Rename inside the staged copy so the manifest name matches the
        // directory (optional — we only assert the directory is loaded).

        // Stage a broken plugin with invalid JSON.
        let broken = root.join("broken-plugin");
        fs::create_dir_all(broken.join(".claude-plugin")).unwrap();
        fs::write(
            broken.join(".claude-plugin").join("plugin.json"),
            "{ this is not valid json",
        )
        .unwrap();

        let repo = fixture_plugin_repo();
        let (plugins, errors) = repo
            .scan_root(root, PluginSource::Project)
            .await
            .expect("scan_root must succeed even when one plugin is broken");

        // The valid plugin must load.
        assert_eq!(
            plugins.len(),
            1,
            "expected exactly one successfully loaded plugin, got {:?}",
            plugins.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
        assert_eq!(plugins[0].name, "bash-logger");

        // The broken plugin must show up in the error tail.
        assert_eq!(
            errors.len(),
            1,
            "expected exactly one plugin load error, got {errors:?}"
        );
        let err = &errors[0];
        assert_eq!(err.plugin_name.as_deref(), Some("broken-plugin"));
        assert!(
            err.error.to_lowercase().contains("parse") || err.error.to_lowercase().contains("json"),
            "error message should mention JSON parsing, got: {}",
            err.error
        );
    }

    /// Wave G-1 Phase 11.1.2 test 3: project-scoped plugins shadow
    /// user-scoped plugins with the same name.
    ///
    /// Stages two tempdir roots — `global/` and `project/` — each
    /// containing a copy of the `bash-logger` fixture. Exercises the real
    /// `scan_root` path for each root, then runs the results through the
    /// private `resolve_plugin_conflicts` helper which is the same
    /// function called by `load_plugins_with_errors`. The project-scoped
    /// copy must win.
    #[tokio::test]
    async fn test_discover_project_shadows_user_same_name() {
        let temp = TempDir::new().unwrap();
        let global_root = temp.path().join("global");
        let project_root = temp.path().join("project");
        fs::create_dir_all(&global_root).unwrap();
        fs::create_dir_all(&project_root).unwrap();

        // Copy the same fixture into both roots.
        let src = wave_g1_fixtures_root().join("bash-logger");
        copy_dir_recursive(&src, &global_root.join("bash-logger")).unwrap();
        copy_dir_recursive(&src, &project_root.join("bash-logger")).unwrap();

        let repo = fixture_plugin_repo();

        // Scan each root with its proper source. `load_plugins_with_errors`
        // uses the same call order (global first, then project) and feeds
        // the concatenated result into `resolve_plugin_conflicts`.
        let (mut combined, mut all_errors): (Vec<LoadedPlugin>, Vec<PluginLoadError>) =
            (Vec::new(), Vec::new());

        let (g_plugins, g_errors) = repo
            .scan_root(&global_root, PluginSource::Global)
            .await
            .expect("scanning global root must succeed");
        combined.extend(g_plugins);
        all_errors.extend(g_errors);

        let (p_plugins, p_errors) = repo
            .scan_root(&project_root, PluginSource::Project)
            .await
            .expect("scanning project root must succeed");
        combined.extend(p_plugins);
        all_errors.extend(p_errors);

        assert!(
            all_errors.is_empty(),
            "no per-plugin errors expected, got {all_errors:?}"
        );
        assert_eq!(
            combined.len(),
            2,
            "expected two copies before conflict resolution"
        );

        // Run through the same helper that `load_plugins_with_errors` uses
        // for precedence resolution.
        let resolved = resolve_plugin_conflicts(combined);

        assert_eq!(
            resolved.len(),
            1,
            "expected exactly one plugin after conflict resolution"
        );
        assert_eq!(resolved[0].name, "bash-logger");
        assert_eq!(
            resolved[0].source,
            PluginSource::Project,
            "project-scoped plugin must shadow the global copy (Project > Global precedence)"
        );
        // The winning plugin's resolved path must be inside the project
        // root, not the global root.
        assert!(
            resolved[0].path.starts_with(&project_root),
            "winning plugin's path should be under the project root, got {:?}",
            resolved[0].path
        );
    }
}
