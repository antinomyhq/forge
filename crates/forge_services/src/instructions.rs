use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_app::{CommandInfra, CustomInstructionsService, EnvironmentInfra, FileReaderInfra};
use forge_domain::{
    InstructionsFrontmatter, InstructionsLoadReason, LoadedInstructions, MemoryType,
};
use gray_matter::Matter;
use gray_matter::engine::YAML;

/// Wave D Pass 1 implementation of [`CustomInstructionsService`].
///
/// Discovers `AGENTS.md` files in three locations in order of priority:
/// 1. Base path ([`forge_domain::Environment::global_agentsmd_path`])
/// 2. Git root directory, when the cwd sits inside a git repository
/// 3. Current working directory
///    ([`forge_domain::Environment::local_agentsmd_path`])
///
/// For each discovered file it reads the body, parses optional YAML
/// frontmatter via `gray_matter`, classifies the source into a
/// [`MemoryType`], and returns a [`LoadedInstructions`] record carrying
/// all of that metadata back to the caller. Pass 1 tags every load
/// reason as [`InstructionsLoadReason::SessionStart`]; the nested
/// traversal, conditional-rule and `@include` reasons are deferred to
/// Pass 2 per
/// `plans/2026-04-09-claude-code-plugins-v4/07-phase-6-t2-infrastructure.md:
/// 343`.
#[derive(Clone)]
pub struct ForgeCustomInstructionsService<F> {
    infra: Arc<F>,
    cache: tokio::sync::OnceCell<Vec<LoadedInstructions>>,
}

impl<F: EnvironmentInfra + FileReaderInfra + CommandInfra> ForgeCustomInstructionsService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, cache: Default::default() }
    }

    async fn discover_agents_files(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let environment = self.infra.get_environment();

        // Base custom instructions
        let base_agent_md = environment.global_agentsmd_path();
        if !paths.contains(&base_agent_md) {
            paths.push(base_agent_md);
        }

        // Repo custom instructions
        if let Some(git_root_path) = self.get_git_root().await {
            let git_agent_md = git_root_path.join("AGENTS.md");
            if !paths.contains(&git_agent_md) {
                paths.push(git_agent_md);
            }
        }

        // Working dir custom instructions
        let cwd_agent_md = environment.local_agentsmd_path();
        if !paths.contains(&cwd_agent_md) {
            paths.push(cwd_agent_md);
        }

        paths
    }

    async fn get_git_root(&self) -> Option<PathBuf> {
        let output = self
            .infra
            .execute_command(
                "git rev-parse --show-toplevel".to_owned(),
                self.infra.get_environment().cwd,
                true, // silent mode - don't print git output
                None, // no environment variables needed for git command
                None, // no extra env vars
            )
            .await
            .ok()?;

        if output.success() {
            Some(PathBuf::from(output.stdout.trim()))
        } else {
            None
        }
    }

    /// Maps a discovered instructions path to its [`MemoryType`]. Pass 1
    /// only distinguishes `User` (base path) from `Project` (everything
    /// else). `Local` and `Managed` are Pass 2 features and are never
    /// returned here.
    fn classify_path(&self, path: &Path) -> MemoryType {
        let environment = self.infra.get_environment();
        let base = environment.global_agentsmd_path();

        if path == base {
            return MemoryType::User;
        }

        // Everything else — git root, cwd, or any future fallback —
        // is treated as project scope. This matches the current
        // 3-file loader's semantics: the global AGENTS.md is the only
        // "user" layer in Pass 1, and both the git-root and cwd
        // AGENTS.md files belong to the project layer.
        MemoryType::Project
    }

    /// Reads a single instructions file off disk and wraps it in a
    /// [`LoadedInstructions`]. Returns `None` when the file cannot be
    /// read (matching the silent-fail behaviour of the previous
    /// implementation) so missing AGENTS.md files don't bubble up as
    /// errors.
    async fn parse_file(&self, path: PathBuf) -> Option<LoadedInstructions> {
        let raw = match self.infra.read_utf8(&path).await {
            Ok(content) => content,
            Err(err) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %err,
                    "skipping instructions file — read failed"
                );
                return None;
            }
        };

        // gray_matter returns the parsed frontmatter in `data` and the
        // body (with the frontmatter block stripped) in `content`. For
        // files without a YAML block `data` is `None` and `content` is
        // the original text verbatim, which is exactly what we want.
        let matter = Matter::<YAML>::new();
        let (frontmatter, content) = match matter.parse::<InstructionsFrontmatter>(&raw) {
            Ok(parsed) => {
                let fm = parsed.data;
                if fm.is_some() {
                    (fm, parsed.content)
                } else {
                    // No frontmatter block at all — preserve the raw
                    // text so downstream callers see the file byte-for-byte.
                    (None, raw)
                }
            }
            Err(err) => {
                // Malformed frontmatter: log and fall back to the raw
                // content so the file is still injected into the
                // context. Do NOT fail the load.
                tracing::debug!(
                    path = %path.display(),
                    error = %err,
                    "instructions frontmatter failed to parse — using raw body"
                );
                (None, raw)
            }
        };

        let memory_type = self.classify_path(&path);
        let globs = frontmatter.as_ref().and_then(|fm| fm.paths.clone());

        Some(LoadedInstructions {
            file_path: path,
            memory_type,
            load_reason: InstructionsLoadReason::SessionStart,
            content,
            frontmatter,
            globs,
            trigger_file_path: None,
            parent_file_path: None,
        })
    }

    async fn init(&self) -> Vec<LoadedInstructions> {
        let paths = self.discover_agents_files().await;

        let mut loaded = Vec::new();
        for path in paths {
            if let Some(entry) = self.parse_file(path).await {
                loaded.push(entry);
            }
        }

        loaded
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + FileReaderInfra + CommandInfra> CustomInstructionsService
    for ForgeCustomInstructionsService<F>
{
    async fn get_custom_instructions_detailed(&self) -> Vec<LoadedInstructions> {
        self.cache.get_or_init(|| self.init()).await.clone()
    }
    // The default `get_custom_instructions` implementation from the
    // trait projects the `content` field out of
    // `get_custom_instructions_detailed`, so we intentionally do not
    // override it here.
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_app::domain::Environment;
    use forge_app::{CommandInfra, CustomInstructionsService, EnvironmentInfra, FileReaderInfra};
    use forge_domain::{
        CommandOutput, ConfigOperation, FileInfo, InstructionsLoadReason, MemoryType,
    };
    use futures::stream;
    use pretty_assertions::assert_eq;

    use super::ForgeCustomInstructionsService;

    /// Mock infra combining [`EnvironmentInfra`], [`FileReaderInfra`]
    /// and [`CommandInfra`] so `ForgeCustomInstructionsService` can be
    /// constructed without pulling in the full forge_infra stack. All
    /// knobs default to "no files, no git repo".
    struct MockInfra {
        base_path: PathBuf,
        cwd: PathBuf,
        /// Map of absolute path → file content. Any path not in the map
        /// yields a "not found" read error, which parse_file translates
        /// into a skipped entry.
        files: Mutex<BTreeMap<PathBuf, String>>,
        /// When `Some`, git rev-parse returns this path as the repo
        /// root. When `None`, git rev-parse fails (matches a cwd that
        /// sits outside any checkout).
        git_root: Option<PathBuf>,
    }

    impl MockInfra {
        fn new(base_path: PathBuf, cwd: PathBuf) -> Self {
            Self {
                base_path,
                cwd,
                files: Mutex::new(BTreeMap::new()),
                git_root: None,
            }
        }

        fn with_file(self, path: PathBuf, content: impl Into<String>) -> Self {
            self.files.lock().unwrap().insert(path, content.into());
            self
        }

        fn with_git_root(mut self, root: PathBuf) -> Self {
            self.git_root = Some(root);
            self
        }
    }

    impl EnvironmentInfra for MockInfra {
        type Config = forge_config::ForgeConfig;

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> BTreeMap<String, String> {
            BTreeMap::new()
        }

        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            let fixture: Environment = Faker.fake();
            fixture
                .base_path(self.base_path.clone())
                .cwd(self.cwd.clone())
        }

        fn get_config(&self) -> anyhow::Result<forge_config::ForgeConfig> {
            Ok(forge_config::ForgeConfig::default())
        }

        async fn update_environment(&self, _ops: Vec<ConfigOperation>) -> anyhow::Result<()> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl FileReaderInfra for MockInfra {
        async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
            let files = self.files.lock().unwrap();
            match files.get(path) {
                Some(content) => Ok(content.clone()),
                None => Err(anyhow::anyhow!("File not found: {path:?}")),
            }
        }

        fn read_batch_utf8(
            &self,
            _: usize,
            _: Vec<PathBuf>,
        ) -> impl futures::Stream<Item = (PathBuf, anyhow::Result<String>)> + Send {
            stream::empty()
        }

        async fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
            let files = self.files.lock().unwrap();
            match files.get(path) {
                Some(content) => Ok(content.as_bytes().to_vec()),
                None => Err(anyhow::anyhow!("File not found: {path:?}")),
            }
        }

        async fn range_read_utf8(
            &self,
            _path: &Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, FileInfo)> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl CommandInfra for MockInfra {
        async fn execute_command(
            &self,
            command: String,
            _working_dir: PathBuf,
            _silent: bool,
            _env_vars: Option<Vec<String>>,
            _extra_env: Option<std::collections::HashMap<String, String>>,
        ) -> anyhow::Result<CommandOutput> {
            // Only `git rev-parse --show-toplevel` is used by the
            // instructions service; every other command should be an
            // unreachable code path in these tests.
            if command == "git rev-parse --show-toplevel" {
                if let Some(root) = self.git_root.as_ref() {
                    return Ok(CommandOutput {
                        stdout: format!("{}\n", root.display()),
                        stderr: String::new(),
                        command,
                        exit_code: Some(0),
                    });
                }
                return Ok(CommandOutput {
                    stdout: String::new(),
                    stderr: "fatal: not a git repository".to_string(),
                    command,
                    exit_code: Some(128),
                });
            }

            unreachable!("unexpected command in instructions test: {command}")
        }

        async fn execute_command_raw(
            &self,
            _command: &str,
            _working_dir: PathBuf,
            _env_vars: Option<Vec<String>>,
            _extra_env: Option<std::collections::HashMap<String, String>>,
        ) -> anyhow::Result<std::process::ExitStatus> {
            unimplemented!()
        }
    }

    // The tests below intentionally pick a cwd and base_path that do
    // not overlap so every discovery path maps to a distinct
    // `PathBuf`. This keeps the 3-file loader's dedup logic out of
    // the way while we exercise classification and frontmatter
    // parsing.
    fn base_path() -> PathBuf {
        PathBuf::from("/home/user/.forge")
    }

    fn cwd() -> PathBuf {
        PathBuf::from("/workspace/project")
    }

    #[tokio::test]
    async fn test_loads_base_agents_md_as_user_memory() {
        // Fixture — only the global ~/.forge/AGENTS.md exists; git is
        // absent and the cwd has no AGENTS.md.
        let infra = MockInfra::new(base_path(), cwd()).with_file(
            base_path().join("AGENTS.md"),
            "# Global rules\n\nBe concise.",
        );
        let service = ForgeCustomInstructionsService::new(std::sync::Arc::new(infra));

        // Act — resolve detailed instructions.
        let actual = service.get_custom_instructions_detailed().await;

        // Assert — exactly one entry, classified as User, tagged
        // `SessionStart`, no frontmatter.
        assert_eq!(actual.len(), 1);
        let entry = &actual[0];
        assert_eq!(entry.file_path, base_path().join("AGENTS.md"));
        assert_eq!(entry.memory_type, MemoryType::User);
        assert_eq!(entry.load_reason, InstructionsLoadReason::SessionStart);
        assert_eq!(entry.content, "# Global rules\n\nBe concise.");
        assert!(entry.frontmatter.is_none());
        assert!(entry.globs.is_none());
        assert!(entry.trigger_file_path.is_none());
        assert!(entry.parent_file_path.is_none());
    }

    #[tokio::test]
    async fn test_loads_project_agents_md_from_git_root() {
        // Fixture — git root reports /workspace/project, git_root
        // AGENTS.md exists, global AGENTS.md does NOT exist. cwd
        // equals git root so the dedup in discover_agents_files
        // prevents a duplicate entry.
        let git_root = PathBuf::from("/workspace/project");
        let infra = MockInfra::new(base_path(), cwd())
            .with_git_root(git_root.clone())
            .with_file(git_root.join("AGENTS.md"), "# Repo rules\n");
        let service = ForgeCustomInstructionsService::new(std::sync::Arc::new(infra));

        // Act.
        let actual = service.get_custom_instructions_detailed().await;

        // Assert — the global base AGENTS.md is silently skipped, and
        // the repo AGENTS.md is classified as Project.
        assert_eq!(actual.len(), 1);
        let entry = &actual[0];
        assert_eq!(entry.file_path, git_root.join("AGENTS.md"));
        assert_eq!(entry.memory_type, MemoryType::Project);
        assert_eq!(entry.load_reason, InstructionsLoadReason::SessionStart);
        assert_eq!(entry.content, "# Repo rules\n");
    }

    #[tokio::test]
    async fn test_parses_frontmatter_with_paths() {
        // Fixture — a global AGENTS.md whose YAML frontmatter sets a
        // `paths` glob. Pass 1 does not act on the glob, but it must
        // parse and surface it via `globs`.
        let content = "---\npaths:\n  - \"*.py\"\n---\nbody";
        let infra =
            MockInfra::new(base_path(), cwd()).with_file(base_path().join("AGENTS.md"), content);
        let service = ForgeCustomInstructionsService::new(std::sync::Arc::new(infra));

        // Act.
        let actual = service.get_custom_instructions_detailed().await;

        // Assert — frontmatter parsed, globs extracted, content has
        // the frontmatter block stripped.
        assert_eq!(actual.len(), 1);
        let entry = &actual[0];
        assert_eq!(entry.memory_type, MemoryType::User);
        assert_eq!(
            entry.globs.as_deref(),
            Some(&["*.py".to_string()][..]),
            "globs must be lifted out of the frontmatter",
        );
        let fm = entry
            .frontmatter
            .as_ref()
            .expect("frontmatter should parse");
        assert_eq!(fm.paths.as_deref(), Some(&["*.py".to_string()][..]));
        assert!(fm.include.is_none());
        // gray_matter strips the frontmatter block but may leave a
        // single trailing newline depending on the input — we assert
        // on the trimmed content to keep the test robust.
        assert_eq!(entry.content.trim(), "body");
    }

    #[tokio::test]
    async fn test_file_without_frontmatter_has_none_frontmatter() {
        // Fixture — a plain markdown file with no YAML block at all.
        let body = "# Plain AGENTS\n\nNothing fancy.";
        let infra =
            MockInfra::new(base_path(), cwd()).with_file(base_path().join("AGENTS.md"), body);
        let service = ForgeCustomInstructionsService::new(std::sync::Arc::new(infra));

        // Act.
        let actual = service.get_custom_instructions_detailed().await;

        // Assert — no frontmatter, no globs, full content preserved.
        assert_eq!(actual.len(), 1);
        let entry = &actual[0];
        assert!(entry.frontmatter.is_none());
        assert!(entry.globs.is_none());
        assert_eq!(entry.content, body);
    }

    #[tokio::test]
    async fn test_missing_file_returns_empty() {
        // Fixture — no files at all, no git repo.
        let infra = MockInfra::new(base_path(), cwd());
        let service = ForgeCustomInstructionsService::new(std::sync::Arc::new(infra));

        // Act.
        let actual = service.get_custom_instructions_detailed().await;

        // Assert — empty vec, and the legacy string projection is
        // also empty so the system prompt builder sees nothing.
        assert_eq!(actual.len(), 0);
        assert!(service.get_custom_instructions().await.is_empty());
    }
}
