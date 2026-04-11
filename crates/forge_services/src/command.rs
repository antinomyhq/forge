use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{Command, CommandSource};
use forge_app::{
    DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra,
    Walker, WalkerInfra,
};
use forge_domain::PluginRepository;
use gray_matter::Matter;
use gray_matter::engine::YAML;

pub struct CommandLoaderService<F> {
    infra: Arc<F>,
    /// Optional plugin repository used to pull commands contributed by
    /// installed plugins.
    plugin_repository: Option<Arc<dyn PluginRepository>>,

    /// In-memory cache of loaded commands.
    ///
    /// Uses [`tokio::sync::RwLock<Option<_>>`] (rather than
    /// [`tokio::sync::OnceCell`]) so that
    /// [`reload`](forge_app::CommandLoaderService::reload) can clear
    /// the cache for Phase 9's `:plugin reload` flow — `OnceCell`
    /// has no public reset API.
    cache: tokio::sync::RwLock<Option<Vec<Command>>>,
}

impl<F> CommandLoaderService<F> {
    /// Production constructor. Wires the plugin repository through so
    /// commands shipped by installed plugins participate in the loader's
    /// merge pipeline.
    pub fn new(infra: Arc<F>, plugin_repository: Arc<dyn PluginRepository>) -> Self {
        Self {
            infra,
            plugin_repository: Some(plugin_repository),
            cache: tokio::sync::RwLock::new(None),
        }
    }

    /// Load built-in commands that are embedded in the application binary.
    fn init_default(&self) -> anyhow::Result<Vec<Command>> {
        let mut commands = parse_command_iter(
            [(
                "github-pr-description",
                include_str!("../../../commands/github-pr-description.md"),
            )]
            .into_iter()
            .map(|(name, content)| (name.to_string(), content.to_string())),
        )?;
        for command in &mut commands {
            command.source = CommandSource::Builtin;
        }
        Ok(commands)
    }
}

#[async_trait::async_trait]
impl<
    F: FileReaderInfra
        + FileWriterInfra
        + FileInfoInfra
        + EnvironmentInfra
        + DirectoryReaderInfra
        + WalkerInfra,
> forge_app::CommandLoaderService for CommandLoaderService<F>
{
    async fn get_commands(&self) -> anyhow::Result<Vec<Command>> {
        self.cache_or_init().await
    }

    /// Clears the in-memory command cache so the next call to
    /// [`get_commands`](forge_app::CommandLoaderService::get_commands)
    /// re-walks the built-in, plugin, global, and project-local
    /// command directories from disk.
    ///
    /// Used by Phase 9's `:plugin reload` flow to surface newly
    /// installed plugin commands without restarting the process.
    async fn reload(&self) -> anyhow::Result<()> {
        let mut guard = self.cache.write().await;
        *guard = None;
        Ok(())
    }
}

impl<
    F: FileReaderInfra
        + FileWriterInfra
        + FileInfoInfra
        + EnvironmentInfra
        + DirectoryReaderInfra
        + WalkerInfra,
> CommandLoaderService<F>
{
    /// Load all command definitions with caching support.
    ///
    /// Implements a double-checked locking pattern: read lock for the fast
    /// path, write lock to repopulate when the cache is empty. Mirrors the
    /// pattern used by `ForgeSkillFetch::get_or_load_skills` and supports
    /// mid-session cache invalidation via
    /// [`reload`](forge_app::CommandLoaderService::reload).
    async fn cache_or_init(&self) -> anyhow::Result<Vec<Command>> {
        // Fast path: read lock, return cached data if present.
        {
            let guard = self.cache.read().await;
            if let Some(commands) = guard.as_ref() {
                return Ok(commands.clone());
            }
        }

        // Slow path: acquire write lock and repopulate.
        let mut guard = self.cache.write().await;
        // Re-check under the write lock in case another task populated it
        // between our read and write acquisitions.
        if let Some(commands) = guard.as_ref() {
            return Ok(commands.clone());
        }

        let commands = self.init().await?;
        *guard = Some(commands.clone());
        Ok(commands)
    }

    async fn init(&self) -> anyhow::Result<Vec<Command>> {
        // Load built-in commands first (lowest precedence)
        let mut commands = self.init_default()?;

        // Plugin commands sit between built-in and user-global custom.
        let plugin_commands = self.load_plugin_commands().await;
        commands.extend(plugin_commands);

        // Load custom commands from global directory
        let dir = self.infra.get_environment().command_path();
        let custom_commands = self
            .init_command_dir(&dir, CommandSource::GlobalUser)
            .await?;
        commands.extend(custom_commands);

        // Load custom commands from CWD
        let dir = self.infra.get_environment().command_path_local();
        let cwd_commands = self
            .init_command_dir(&dir, CommandSource::ProjectCwd)
            .await?;

        commands.extend(cwd_commands);

        // Handle command name conflicts by keeping the last occurrence
        // This gives precedence order: CWD > Global Custom > Plugin > Built-in
        Ok(resolve_command_conflicts(commands))
    }

    async fn init_command_dir(
        &self,
        dir: &std::path::Path,
        source: CommandSource,
    ) -> anyhow::Result<Vec<Command>> {
        if !self.infra.exists(dir).await? {
            return Ok(vec![]);
        }

        // Use DirectoryReaderInfra to read all .md files in parallel
        let files = self
            .infra
            .read_directory_files(dir, Some("*.md"))
            .await
            .with_context(|| format!("Failed to read commands from: {}", dir.display()))?;

        let mut commands = parse_command_iter(files.into_iter().map(|(path, content)| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            (name, content)
        }))?;

        for command in &mut commands {
            command.source = source.clone();
        }

        Ok(commands)
    }

    /// Loads commands from every enabled plugin returned by the injected
    /// [`PluginRepository`]. Returns an empty vector when no plugin
    /// repository is wired in.
    async fn load_plugin_commands(&self) -> Vec<Command> {
        let Some(plugin_repo) = self.plugin_repository.as_ref() else {
            return Vec::new();
        };

        let plugins = match plugin_repo.load_plugins().await {
            Ok(plugins) => plugins,
            Err(err) => {
                tracing::warn!("Failed to enumerate plugins for command loading: {err:#}");
                return Vec::new();
            }
        };

        let mut all = Vec::new();
        for plugin in plugins.into_iter().filter(|p| p.enabled) {
            for commands_dir in &plugin.commands_paths {
                match self
                    .init_plugin_command_dir(commands_dir, &plugin.name)
                    .await
                {
                    Ok(loaded) => all.extend(loaded),
                    Err(err) => {
                        tracing::warn!(
                            "Failed to load plugin commands from {}: {err:#}",
                            commands_dir.display()
                        );
                    }
                }
            }
        }

        all
    }

    /// Recursively walks a plugin `commands_dir` and produces a list of
    /// [`Command`]s whose names encode nested directory structure with `:`
    /// separators, e.g. `commands/git/commit.md` under plugin `demo`
    /// becomes `demo:git:commit`.
    async fn init_plugin_command_dir(
        &self,
        dir: &Path,
        plugin_name: &str,
    ) -> anyhow::Result<Vec<Command>> {
        if !self.infra.exists(dir).await? {
            return Ok(vec![]);
        }

        let walker = Walker::unlimited().cwd(dir.to_path_buf());
        let entries = self
            .infra
            .walk(walker)
            .await
            .with_context(|| format!("Failed to walk plugin command dir: {}", dir.display()))?;

        let mut commands = Vec::new();
        for walked in entries {
            if walked.is_dir() || walked.path.is_empty() {
                continue;
            }

            // Relative path segment like "git/commit.md" or "deploy.md".
            let rel_path = std::path::PathBuf::from(&walked.path);
            let is_md = rel_path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("md"));
            if !is_md {
                continue;
            }

            let full_path = dir.join(&rel_path);
            let content = match self.infra.read_utf8(&full_path).await {
                Ok(c) => c,
                Err(err) => {
                    tracing::warn!(
                        "Failed to read plugin command file {}: {err:#}",
                        full_path.display()
                    );
                    continue;
                }
            };

            let namespaced_name = plugin_namespaced_command_name(plugin_name, &rel_path);

            let mut command = match parse_command_file(&content) {
                Ok(cmd) => cmd,
                Err(err) => {
                    tracing::warn!(
                        "Failed to parse plugin command {}: {err:#}",
                        full_path.display()
                    );
                    continue;
                }
            };
            command.name = namespaced_name;
            command.source = CommandSource::Plugin { plugin_name: plugin_name.to_string() };
            commands.push(command);
        }

        Ok(commands)
    }
}

/// Converts a relative command path into a namespaced command name with
/// `:` separators. Examples:
///
/// - `deploy.md` under plugin `demo` → `demo:deploy`
/// - `git/commit.md` under plugin `demo` → `demo:git:commit`
/// - `review/deep/critical.md` under plugin `demo` →
///   `demo:review:deep:critical`
fn plugin_namespaced_command_name(plugin_name: &str, rel_path: &Path) -> String {
    let mut segments: Vec<String> = Vec::new();
    segments.push(plugin_name.to_string());

    let mut components: Vec<String> = rel_path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
            _ => None,
        })
        .collect();

    if let Some(last) = components.last_mut() {
        // Strip the trailing `.md` extension from the filename before
        // joining, keeping directory names as-is.
        if let Some(stripped) = last
            .strip_suffix(".md")
            .or_else(|| last.strip_suffix(".MD"))
        {
            *last = stripped.to_string();
        }
    }

    segments.extend(components);
    segments.join(":")
}

/// Implementation function for resolving command name conflicts by keeping the
/// last occurrence. This implements the precedence order: CWD Custom > Global
/// Custom > Plugin > Built-in
fn resolve_command_conflicts(commands: Vec<Command>) -> Vec<Command> {
    // Use HashMap to deduplicate by command name, keeping the last occurrence
    let mut command_map: HashMap<String, Command> = HashMap::new();

    for command in commands {
        command_map.insert(command.name.clone(), command);
    }

    // Convert back to vector (order is not guaranteed but doesn't matter for the
    // service)
    command_map.into_values().collect()
}

fn parse_command_iter<I, Path: AsRef<str>, Content: AsRef<str>>(
    contents: I,
) -> anyhow::Result<Vec<Command>>
where
    I: Iterator<Item = (Path, Content)>,
{
    let mut commands = vec![];

    for (name, content) in contents {
        let command = parse_command_file(content.as_ref())
            .with_context(|| format!("Failed to parse command: {}", name.as_ref()))?;

        commands.push(command);
    }

    Ok(commands)
}

/// Parse raw content into a Command with YAML frontmatter
fn parse_command_file(content: &str) -> Result<Command> {
    // Parse the frontmatter using gray_matter with type-safe deserialization
    let gray_matter = Matter::<YAML>::new();
    let result = gray_matter.parse::<Command>(content)?;

    // Extract the frontmatter
    let command = result
        .data
        .context("Empty command frontmatter")?
        .prompt(result.content);

    Ok(command)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_domain::{LoadedPlugin, PluginLoadResult, PluginManifest, PluginSource};
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_parse_basic_command() {
        let content = forge_test_kit::fixture!("src/fixtures/commands/basic.md").await;

        let actual = parse_command_file(&content).unwrap();

        assert_eq!(actual.name.as_str(), "test-basic");
        assert_eq!(actual.description.as_str(), "A basic test command");
        assert_eq!(
            actual.prompt.as_ref().unwrap(),
            "This is the prompt content for the basic test command."
        );
    }

    #[tokio::test]
    async fn test_parse_command_with_multiline_prompt() {
        let content = forge_test_kit::fixture!("src/fixtures/commands/multiline.md").await;

        let actual = parse_command_file(&content).unwrap();

        assert_eq!(actual.name.as_str(), "test-multiline");
        assert_eq!(actual.description.as_str(), "Command with multiline prompt");
        assert!(actual.prompt.as_ref().unwrap().contains("Step 1"));
        assert!(actual.prompt.as_ref().unwrap().contains("Step 2"));
    }

    #[tokio::test]
    async fn test_parse_invalid_frontmatter() {
        let content = forge_test_kit::fixture!("src/fixtures/commands/invalid.md").await;

        let result = parse_command_file(&content);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_builtin_commands() {
        // Test that all built-in commands parse correctly
        let builtin_commands = [
            ("fixme", "../../.forge/commands/fixme.md"),
            ("check", "../../.forge/commands/check.md"),
        ];

        for (name, path) in builtin_commands {
            let content = forge_test_kit::fixture!(path).await;
            let command = parse_command_file(&content)
                .with_context(|| format!("Failed to parse built-in command: {}", name))
                .unwrap();

            assert_eq!(command.name.as_str(), name);
            assert!(!command.description.is_empty());
            assert!(command.prompt.is_some());
        }
    }

    #[test]
    fn test_init_default_contains_builtin_commands() {
        // Fixture
        let service = CommandLoaderService::<()> {
            infra: Arc::new(()),
            plugin_repository: None,
            cache: tokio::sync::RwLock::new(None),
        };

        // Execute
        let actual = service.init_default().unwrap();

        // Verify github-pr-description
        let command = actual
            .iter()
            .find(|c| c.name.as_str() == "github-pr-description")
            .expect("github-pr-description should be a built-in command");

        assert_eq!(command.name.as_str(), "github-pr-description");
        assert!(!command.description.is_empty());
        assert!(command.prompt.is_some());
        assert_eq!(command.source, CommandSource::Builtin);
    }

    #[test]
    fn test_resolve_command_conflicts_no_duplicates() {
        let fixture = vec![
            Command::default().name("command1").description("Command 1"),
            Command::default().name("command2").description("Command 2"),
            Command::default().name("command3").description("Command 3"),
        ];

        let actual = resolve_command_conflicts(fixture.clone());

        // Should return all commands when no conflicts
        assert_eq!(actual.len(), 3);

        let names: std::collections::HashSet<_> = actual.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains("command1"));
        assert!(names.contains("command2"));
        assert!(names.contains("command3"));
    }

    #[test]
    fn test_resolve_command_conflicts_with_duplicates() {
        let fixture = vec![
            Command::default()
                .name("command1")
                .description("Global Command 1"),
            Command::default()
                .name("command2")
                .description("Global Command 2"),
            Command::default()
                .name("command1")
                .description("CWD Command 1 - Override"), // Duplicate name, should override
            Command::default()
                .name("command3")
                .description("CWD Command 3"),
        ];

        let actual = resolve_command_conflicts(fixture);

        // Should have 3 commands: command1 (CWD version), command2 (global), command3
        // (CWD)
        assert_eq!(actual.len(), 3);

        let command1 = actual
            .iter()
            .find(|c| c.name.as_str() == "command1")
            .expect("Should have command1");
        let expected_description = "CWD Command 1 - Override";
        assert_eq!(command1.description.as_str(), expected_description);
    }

    #[test]
    fn test_resolve_command_conflicts_multiple_duplicates() {
        // Test scenario: Built-in -> Global -> CWD (CWD should win)
        let fixture = vec![
            Command::default()
                .name("common")
                .description("Built-in Common Command"),
            Command::default()
                .name("unique1")
                .description("Built-in Unique 1"),
            Command::default()
                .name("common")
                .description("Global Common Command"), // Override built-in
            Command::default()
                .name("unique2")
                .description("Global Unique 2"),
            Command::default()
                .name("common")
                .description("CWD Common Command"), // Override global
            Command::default()
                .name("unique3")
                .description("CWD Unique 3"),
        ];

        let actual = resolve_command_conflicts(fixture);

        // Should have 4 commands: common (CWD version), unique1, unique2, unique3
        assert_eq!(actual.len(), 4);

        let common = actual
            .iter()
            .find(|c| c.name.as_str() == "common")
            .expect("Should have common command");
        let expected_description = "CWD Common Command";
        assert_eq!(common.description.as_str(), expected_description);

        // Verify all unique commands are present
        let names: std::collections::HashSet<_> = actual.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains("common"));
        assert!(names.contains("unique1"));
        assert!(names.contains("unique2"));
        assert!(names.contains("unique3"));
    }

    #[test]
    fn test_resolve_command_conflicts_empty_input() {
        let fixture: Vec<Command> = vec![];

        let actual = resolve_command_conflicts(fixture);

        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_plugin_namespaced_command_name_top_level() {
        let rel_path = PathBuf::from("deploy.md");
        let actual = plugin_namespaced_command_name("demo", &rel_path);
        assert_eq!(actual, "demo:deploy");
    }

    #[test]
    fn test_plugin_namespaced_command_name_single_nesting() {
        let rel_path = PathBuf::from("git/commit.md");
        let actual = plugin_namespaced_command_name("demo", &rel_path);
        assert_eq!(actual, "demo:git:commit");
    }

    #[test]
    fn test_plugin_namespaced_command_name_deep_nesting() {
        let rel_path = PathBuf::from("review/deep/critical.md");
        let actual = plugin_namespaced_command_name("demo", &rel_path);
        assert_eq!(actual, "demo:review:deep:critical");
    }

    #[test]
    fn test_plugin_namespaced_command_name_case_insensitive_md() {
        let rel_path = PathBuf::from("deploy.MD");
        let actual = plugin_namespaced_command_name("demo", &rel_path);
        assert_eq!(actual, "demo:deploy");
    }

    /// Test-only in-memory [`PluginRepository`] used to feed the command
    /// loader a fixed plugin list.
    struct MockPluginRepository {
        plugins: Vec<LoadedPlugin>,
        load_count: std::sync::atomic::AtomicUsize,
    }

    impl MockPluginRepository {
        fn new(plugins: Vec<LoadedPlugin>) -> Self {
            Self { plugins, load_count: std::sync::atomic::AtomicUsize::new(0) }
        }

        fn load_count(&self) -> usize {
            self.load_count.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl PluginRepository for MockPluginRepository {
        async fn load_plugins(&self) -> anyhow::Result<Vec<LoadedPlugin>> {
            self.load_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.plugins.clone())
        }

        async fn load_plugins_with_errors(&self) -> anyhow::Result<PluginLoadResult> {
            Ok(PluginLoadResult::new(self.plugins.clone(), Vec::new()))
        }
    }

    fn fixture_plugin(name: &str, enabled: bool, commands_path: PathBuf) -> LoadedPlugin {
        LoadedPlugin {
            name: name.to_string(),
            manifest: PluginManifest { name: Some(name.to_string()), ..Default::default() },
            path: PathBuf::from(format!("/fake/{name}")),
            source: PluginSource::Global,
            enabled,
            is_builtin: false,
            commands_paths: vec![commands_path],
            agents_paths: Vec::new(),
            skills_paths: Vec::new(),
            mcp_servers: None,
        }
    }

    /// Minimal filesystem-backed infra used only by plugin command tests.
    ///
    /// Implements the handful of infra traits that
    /// [`CommandLoaderService::load_plugin_commands`] touches:
    /// `FileInfoInfra::exists`, `FileReaderInfra::read_utf8`, and
    /// `WalkerInfra::walk`. All other trait methods are either stubbed
    /// or unreachable for the scenarios under test.
    #[derive(Clone, Default)]
    struct PluginFsInfra;

    #[async_trait::async_trait]
    impl forge_app::FileInfoInfra for PluginFsInfra {
        async fn is_binary(&self, _path: &std::path::Path) -> anyhow::Result<bool> {
            Ok(false)
        }
        async fn is_file(&self, path: &std::path::Path) -> anyhow::Result<bool> {
            Ok(tokio::fs::metadata(path)
                .await
                .map(|m| m.is_file())
                .unwrap_or(false))
        }
        async fn exists(&self, path: &std::path::Path) -> anyhow::Result<bool> {
            Ok(tokio::fs::try_exists(path).await.unwrap_or(false))
        }
        async fn file_size(&self, path: &std::path::Path) -> anyhow::Result<u64> {
            Ok(tokio::fs::metadata(path).await?.len())
        }
    }

    #[async_trait::async_trait]
    impl forge_app::FileReaderInfra for PluginFsInfra {
        async fn read_utf8(&self, path: &std::path::Path) -> anyhow::Result<String> {
            Ok(tokio::fs::read_to_string(path).await?)
        }

        fn read_batch_utf8(
            &self,
            _batch_size: usize,
            _paths: Vec<PathBuf>,
        ) -> impl futures::Stream<Item = (PathBuf, anyhow::Result<String>)> + Send {
            futures::stream::empty()
        }

        async fn read(&self, path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
            Ok(tokio::fs::read(path).await?)
        }

        async fn range_read_utf8(
            &self,
            _path: &std::path::Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, forge_domain::FileInfo)> {
            unreachable!("range_read_utf8 is not used by plugin command loading")
        }
    }

    #[async_trait::async_trait]
    impl forge_app::FileWriterInfra for PluginFsInfra {
        async fn write(
            &self,
            _path: &std::path::Path,
            _contents: bytes::Bytes,
        ) -> anyhow::Result<()> {
            unreachable!("write is not used by plugin command loading")
        }
        async fn append(
            &self,
            _path: &std::path::Path,
            _contents: bytes::Bytes,
        ) -> anyhow::Result<()> {
            unreachable!("append is not used by plugin command loading")
        }
        async fn write_temp(
            &self,
            _prefix: &str,
            _ext: &str,
            _content: &str,
        ) -> anyhow::Result<PathBuf> {
            unreachable!("write_temp is not used by plugin command loading")
        }
    }

    impl forge_app::EnvironmentInfra for PluginFsInfra {
        type Config = forge_config::ForgeConfig;

        fn get_environment(&self) -> forge_domain::Environment {
            use fake::{Fake, Faker};
            Faker.fake()
        }

        fn get_config(&self) -> anyhow::Result<forge_config::ForgeConfig> {
            Ok(forge_config::ForgeConfig::default())
        }

        async fn update_environment(
            &self,
            _ops: Vec<forge_domain::ConfigOperation>,
        ) -> anyhow::Result<()> {
            unreachable!("update_environment is not used by plugin command loading")
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> std::collections::BTreeMap<String, String> {
            std::collections::BTreeMap::new()
        }
    }

    #[async_trait::async_trait]
    impl forge_app::DirectoryReaderInfra for PluginFsInfra {
        async fn list_directory_entries(
            &self,
            _directory: &std::path::Path,
        ) -> anyhow::Result<Vec<(PathBuf, bool)>> {
            Ok(Vec::new())
        }

        async fn read_directory_files(
            &self,
            _directory: &std::path::Path,
            _pattern: Option<&str>,
        ) -> anyhow::Result<Vec<(PathBuf, String)>> {
            Ok(Vec::new())
        }
    }

    #[async_trait::async_trait]
    impl forge_app::WalkerInfra for PluginFsInfra {
        async fn walk(&self, config: Walker) -> anyhow::Result<Vec<forge_app::WalkedFile>> {
            // Reuse the real walker implementation by delegating to
            // `forge_walker::Walker` so plugin command tests exercise the
            // same recursion semantics as production.
            let root = config.cwd.clone();
            let mut files = Vec::new();
            walk_dir_recursive(&root, &root, &mut files).await?;
            Ok(files)
        }
    }

    async fn walk_dir_recursive(
        root: &std::path::Path,
        current: &std::path::Path,
        out: &mut Vec<forge_app::WalkedFile>,
    ) -> anyhow::Result<()> {
        let mut read_dir = match tokio::fs::read_dir(current).await {
            Ok(rd) => rd,
            Err(_) => return Ok(()),
        };
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            let rel_raw = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let metadata = entry.metadata().await?;
            if metadata.is_dir() {
                // `WalkedFile::is_dir` treats paths ending in `/` as
                // directories; mirror that convention here.
                let rel = format!("{rel_raw}/");
                out.push(forge_app::WalkedFile { path: rel, file_name: Some(file_name), size: 0 });
                Box::pin(walk_dir_recursive(root, &path, out)).await?;
            } else {
                out.push(forge_app::WalkedFile {
                    path: rel_raw,
                    file_name: Some(file_name),
                    size: metadata.len(),
                });
            }
        }
        Ok(())
    }

    fn fixture_command_loader_with_plugins(
        plugins: Vec<LoadedPlugin>,
    ) -> CommandLoaderService<PluginFsInfra> {
        let infra = Arc::new(PluginFsInfra);
        let plugin_repo: Arc<dyn PluginRepository> = Arc::new(MockPluginRepository::new(plugins));
        CommandLoaderService::new(infra, plugin_repo)
    }

    fn fixture_command_loader_with_mock(
        mock: Arc<MockPluginRepository>,
    ) -> CommandLoaderService<PluginFsInfra> {
        let infra = Arc::new(PluginFsInfra);
        // Adapter wrapper so the loader sees `Arc<dyn PluginRepository>` while
        // the test still holds an `Arc<MockPluginRepository>` for assertions.
        struct MockAdapter(Arc<MockPluginRepository>);

        #[async_trait::async_trait]
        impl PluginRepository for MockAdapter {
            async fn load_plugins(&self) -> anyhow::Result<Vec<LoadedPlugin>> {
                self.0.load_plugins().await
            }
            async fn load_plugins_with_errors(&self) -> anyhow::Result<PluginLoadResult> {
                self.0.load_plugins_with_errors().await
            }
        }

        let adapter: Arc<dyn PluginRepository> = Arc::new(MockAdapter(mock));
        CommandLoaderService::new(infra, adapter)
    }

    #[tokio::test]
    async fn test_load_plugin_commands_top_level_and_nested_namespace() {
        let commands_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/plugin_commands");
        let plugin = fixture_plugin("demo", true, commands_dir);
        let service = fixture_command_loader_with_plugins(vec![plugin]);

        let loaded = service.load_plugin_commands().await;

        // Expect exactly four commands from the fixture tree:
        //   commands/deploy.md             -> demo:deploy
        //   commands/git/commit.md         -> demo:git:commit
        //   commands/review/deep/critical.md -> demo:review:deep:critical
        //   commands/nested.md             -> demo:nested
        let names: std::collections::HashSet<_> = loaded.iter().map(|c| c.name.clone()).collect();
        assert!(names.contains("demo:deploy"), "names={names:?}");
        assert!(names.contains("demo:git:commit"), "names={names:?}");
        assert!(
            names.contains("demo:review:deep:critical"),
            "names={names:?}"
        );
        assert!(names.contains("demo:nested"), "names={names:?}");

        // Every loaded command must carry the plugin source tag and preserve
        // its frontmatter description.
        for command in &loaded {
            assert_eq!(
                command.source,
                CommandSource::Plugin { plugin_name: "demo".to_string() }
            );
            assert!(!command.description.is_empty());
        }
    }

    #[tokio::test]
    async fn test_load_plugin_commands_skips_disabled_plugins() {
        let commands_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/plugin_commands");
        let plugin = fixture_plugin("demo", false, commands_dir);
        let service = fixture_command_loader_with_plugins(vec![plugin]);

        let loaded = service.load_plugin_commands().await;
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn test_load_plugin_commands_handles_missing_dir() {
        let missing = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fixtures/definitely-does-not-exist");
        let plugin = fixture_plugin("demo", true, missing);
        let service = fixture_command_loader_with_plugins(vec![plugin]);

        let loaded = service.load_plugin_commands().await;
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn test_get_commands_caches_across_calls() {
        // Fixture: with a single plugin source, repeated `get_commands`
        // calls must hit the plugin repository exactly once thanks to the
        // RwLock-based cache.
        let commands_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/plugin_commands");
        let plugin = fixture_plugin("demo", true, commands_dir);
        let mock = Arc::new(MockPluginRepository::new(vec![plugin]));
        let service = fixture_command_loader_with_mock(mock.clone());

        // Act
        use forge_app::CommandLoaderService as _;
        let _ = service.get_commands().await.unwrap();
        let _ = service.get_commands().await.unwrap();
        let _ = service.get_commands().await.unwrap();

        // Assert
        assert_eq!(mock.load_count(), 1);
    }

    #[tokio::test]
    async fn test_reload_clears_command_cache() {
        // Fixture: prime the cache, then call `reload`, and verify the
        // next `get_commands` call hits the plugin repository again.
        let commands_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/plugin_commands");
        let plugin = fixture_plugin("demo", true, commands_dir);
        let mock = Arc::new(MockPluginRepository::new(vec![plugin]));
        let service = fixture_command_loader_with_mock(mock.clone());

        use forge_app::CommandLoaderService as _;

        // Prime the cache
        let _ = service.get_commands().await.unwrap();
        assert_eq!(mock.load_count(), 1);

        // Act
        service.reload().await.unwrap();
        let _ = service.get_commands().await.unwrap();

        // Assert: exactly one additional repository hit
        assert_eq!(mock.load_count(), 2);
    }
}
