use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use forge_app::dto::ToolsOverview;
use forge_app::hook_runtime::HookConfigLoaderService;
use forge_app::{
    AgentProviderResolver, AgentRegistry, AppConfigService, AuthService, CommandInfra,
    CommandLoaderService, ConversationService, DataGenerationApp, EnvironmentInfra,
    FileDiscoveryService, ForgeApp, ForgeNotificationService, GitApp, GrpcInfra, McpConfigManager,
    McpService, NotificationService, PluginComponentsReloader, PluginLoader, ProviderAuthService,
    ProviderService, Services, User, UserUsage, Walker, WorkspaceService, fire_setup_hook,
};
use forge_config::{ConfigReader, ForgeConfig};
use forge_domain::{Agent, ConsoleWriter, HookEventName, *};
use forge_infra::ForgeInfra;
use forge_repo::ForgeRepo;
use forge_services::{ForgeServices, RecursiveMode};
use forge_stream::MpscStream;
use futures::stream::BoxStream;
use tokio::runtime::Handle;
use tracing::warn;
use url::Url;

use crate::API;
use crate::config_watcher_handle::ConfigWatcherHandle;
use crate::file_changed_watcher_handle::FileChangedWatcherHandle;

pub struct ForgeAPI<S, F> {
    services: Arc<S>,
    infra: Arc<F>,
    /// Background filesystem watcher that fires the `ConfigChange`
    /// lifecycle hook when Forge's config files / plugin directory
    /// change on disk. `None` when construction failed or the
    /// watcher was disabled (e.g. unit tests). Prefixed with an
    /// underscore because the field is kept alive purely for the
    /// inner `Arc<ConfigWatcher>`'s `Drop` impl — nothing reads
    /// the field directly on the generic `impl<A, F>` block.
    ///
    /// The concrete `init` path also exposes a clone of this handle
    /// to internal call sites (e.g. `set_plugin_enabled`) via
    /// [`ForgeAPI::mark_config_write`] so Forge's own writes can be
    /// suppressed within the 5-second internal-write window.
    _config_watcher: Option<ConfigWatcherHandle>,
    /// Background filesystem watcher that fires the `FileChanged`
    /// lifecycle hook (Phase 7C Wave E-2a) when any user-configured
    /// watched file changes on disk. `None` when construction failed,
    /// no `FileChanged` matchers are present in the merged hook
    /// config, or the call site lacked a multi-threaded tokio runtime
    /// to bootstrap the async config loader. Prefixed with an
    /// underscore for the same Drop-impl-lifetime reason as
    /// `_config_watcher`.
    _file_changed_watcher: Option<FileChangedWatcherHandle>,
}

impl<A, F> ForgeAPI<A, F> {
    pub fn new(services: Arc<A>, infra: Arc<F>) -> Self {
        Self {
            services,
            infra,
            _config_watcher: None,
            _file_changed_watcher: None,
        }
    }

    /// Returns a clone of the internal services `Arc`.
    ///
    /// Used by the `--worktree` CLI flag handler in
    /// `crates/forge_main/src/main.rs` to fire the
    /// `WorktreeCreate` plugin hook via
    /// [`forge_app::fire_worktree_create_hook`] before the main
    /// orchestrator run begins. The services Arc is shared across
    /// the whole API — cloning it here is the same
    /// reference-counted clone the internal `app()` helper uses.
    pub fn services(&self) -> Arc<A> {
        self.services.clone()
    }

    /// Creates a ForgeApp instance with the current services and latest config.
    fn app(&self) -> ForgeApp<A>
    where
        A: Services + EnvironmentInfra<Config = forge_config::ForgeConfig>,
        F: EnvironmentInfra<Config = forge_config::ForgeConfig>,
    {
        ForgeApp::new(self.services.clone())
    }
}

impl ForgeAPI<ForgeServices<ForgeRepo<ForgeInfra>>, ForgeRepo<ForgeInfra>> {
    /// Creates a fully-initialized [`ForgeAPI`] from a pre-read configuration.
    ///
    /// # Arguments
    /// * `cwd` - The working directory path for environment and file resolution
    /// * `config` - Pre-read application configuration (from startup)
    /// * `services_url` - Pre-validated URL for the gRPC workspace server
    pub fn init(cwd: PathBuf, config: ForgeConfig, services_url: Url) -> Self {
        let infra = Arc::new(ForgeInfra::new(cwd, config, services_url));
        let repo = Arc::new(ForgeRepo::new(infra.clone()));
        let app = Arc::new(ForgeServices::new(repo.clone()));

        // Phase 8 Wave F-1: populate the elicitation dispatcher's
        // late-init `Arc<Self>` slot now that the services aggregate
        // exists. The dispatcher needs a handle back to `app` to fire
        // the `Elicitation` plugin hook, but storing `Arc<Self>`
        // inside `ForgeServices::new` would create a chicken-and-egg
        // cycle (the `Arc` doesn't exist until `Arc::new` returns).
        // This `init_elicitation_dispatcher` call closes that cycle
        // exactly once; until it runs, the dispatcher declines every
        // request with a warn log.
        app.init_elicitation_dispatcher();

        // Populate the hook executor's LLM service handle so prompt
        // and agent hooks can make model calls. Same OnceLock pattern
        // as the elicitation dispatcher — must run after `Arc::new`.
        app.init_hook_executor_services();

        // Phase 8 Wave F-2: plumb the same dispatcher into
        // `ForgeInfra`'s `ForgeMcpServer` slot so the rmcp
        // `ClientHandler::create_elicitation` callback (implemented
        // by `forge_infra::ForgeMcpHandler`) can route MCP
        // server-initiated elicitation requests through the plugin
        // hook pipeline. This must run AFTER
        // `app.init_elicitation_dispatcher()` so the
        // `ForgeElicitationDispatcher` inside the returned Arc has a
        // live `Services` handle; otherwise every MCP elicitation
        // would decline with a "called before init" warn log.
        infra.init_elicitation_dispatcher(app.elicitation_dispatcher_arc());

        // Wave C Part 2: spin up the `ConfigWatcher` that feeds the
        // `ConfigChange` lifecycle hook. The watch paths are derived
        // from the live `Environment`:
        //
        // - `base_path` (NonRecursive) covers `~/forge/.forge.toml` and any other
        //   top-level config files that sit directly inside the Forge config directory.
        // - `plugin_path` (Recursive) covers `~/forge/plugins/**` so any
        //   add/remove/edit inside an installed plugin fires a `ConfigChange { source:
        //   Plugins, .. }` event.
        //
        // The watcher itself skips paths that do not exist yet
        // (logged at `debug!`), so we can blindly include
        // `plugin_path()` even on a fresh install.
        let environment = app.get_environment();
        let watch_paths: Vec<(PathBuf, RecursiveMode)> = vec![
            (environment.base_path.clone(), RecursiveMode::NonRecursive),
            (environment.plugin_path(), RecursiveMode::Recursive),
        ];

        // Build the watcher handle. On construction failure we log a
        // warning and fall back to `None` so the API still boots —
        // `ConfigChange` is an observability event, not a correctness
        // event, so losing it must not be fatal.
        let config_watcher = match ConfigWatcherHandle::spawn(app.clone(), watch_paths) {
            Ok(handle) => Some(handle),
            Err(err) => {
                warn!(
                    error = %err,
                    "failed to start ConfigWatcher; ConfigChange hooks will be disabled"
                );
                None
            }
        };

        // Phase 7C Wave E-2a: spin up the `FileChangedWatcher` that
        // feeds the `FileChanged` lifecycle hook. Unlike the config
        // watcher — which derives its paths purely from the live
        // `Environment` — this one has to load the merged hook
        // config asynchronously so it can discover the user's
        // `FileChanged` matchers (e.g. `.envrc|.env` in a
        // `hooks.json`). `ForgeAPI::init` is sync, so we need to
        // bridge async→sync. We do it with `block_in_place` +
        // `Handle::block_on`, but ONLY on a multi-thread tokio
        // runtime where that pattern is safe. On a single-thread
        // runtime (or outside any runtime) we silently skip the
        // watcher; the single-thread case exists mostly in unit
        // tests, where `FileChanged` observability is not required.
        //
        // TODO(wave-e-2a): converting `ForgeAPI::init` to async would
        // let us drop the block_in_place dance entirely. That's a
        // cross-cutting change tracked separately.
        let file_changed_watcher = match Handle::try_current() {
            Ok(runtime)
                if runtime.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread =>
            {
                let services_for_load = app.clone();
                let watch_paths = tokio::task::block_in_place(move || {
                    runtime.block_on(resolve_file_changed_watch_paths(services_for_load))
                });

                // Phase 7C Wave E-2b: we spawn the watcher even when
                // the startup resolver returned no matchers so that
                // runtime `watch_paths` from `SessionStart` hooks
                // still have a live watcher to install against. An
                // empty-paths `FileChangedWatcher` just sits idle
                // until `add_paths` is called later.
                match FileChangedWatcherHandle::spawn(app.clone(), watch_paths) {
                    Ok(handle) => {
                        // Register the handle so the orchestrator's
                        // `SessionStart` fire site can push dynamic
                        // `watch_paths` into it via
                        // `forge_app::add_file_changed_watch_paths`.
                        // `install_file_changed_watcher_ops` is a
                        // `OnceLock::set` under the hood, so a second
                        // `ForgeAPI::init` call (rare — only in tests
                        // that spin up multiple APIs in the same
                        // process) is a silent no-op.
                        forge_app::install_file_changed_watcher_ops(Arc::new(handle.clone()));
                        Some(handle)
                    }
                    Err(err) => {
                        warn!(
                            error = %err,
                            "failed to start FileChangedWatcher; FileChanged hooks will be disabled"
                        );
                        None
                    }
                }
            }
            _ => {
                // Single-thread runtime or no runtime at all —
                // `block_in_place` would panic on the former and
                // `block_on` on the latter. Silently skip the
                // watcher; the test harness is the only expected
                // caller here.
                None
            }
        };

        ForgeAPI {
            services: app,
            infra: repo,
            _config_watcher: config_watcher,
            _file_changed_watcher: file_changed_watcher,
        }
    }

    pub async fn get_skills_internal(&self) -> Result<Vec<Skill>> {
        use forge_domain::SkillRepository;
        self.infra.load_skills().await
    }
}

/// Resolve the list of filesystem paths the `FileChangedWatcher`
/// should observe, derived from the `FileChanged` matchers in the
/// merged hook config.
///
/// Claude Code accepts pipe-separated alternatives (e.g.
/// `".envrc|.env"`) inside a single matcher string. We split on `|`,
/// trim each entry, resolve relative paths against
/// `environment.cwd`, and drop any entry whose resolved path does
/// not exist on disk — the watcher skips missing paths internally
/// too, but filtering here keeps the watcher's install log quiet.
///
/// All entries are installed with [`RecursiveMode::NonRecursive`]
/// because the Claude Code wire semantics treat a matcher as a
/// single file path, not a directory tree. Users who want
/// recursive behaviour can supply `*` globs in their hook command
/// bodies and filter inside the hook itself.
async fn resolve_file_changed_watch_paths<S: Services + 'static>(
    services: Arc<S>,
) -> Vec<(PathBuf, RecursiveMode)> {
    use crate::file_changed_watcher_handle::parse_file_changed_matcher;

    let merged = match services.hook_config_loader().load().await {
        Ok(config) => config,
        Err(err) => {
            warn!(
                error = %err,
                "failed to load merged hook config for FileChangedWatcher; \
                 FileChanged hooks will be disabled"
            );
            return Vec::new();
        }
    };

    let Some(matchers) = merged.entries.get(&HookEventName::FileChanged) else {
        return Vec::new();
    };

    let cwd = services.get_environment().cwd;
    let mut result: Vec<(PathBuf, RecursiveMode)> = Vec::new();

    for matcher_with_source in matchers {
        let Some(pattern) = matcher_with_source.matcher.matcher.as_deref() else {
            continue;
        };

        // Delegate the split-on-pipe / cwd-resolve logic to the shared
        // helper so the runtime consumer in `orch.rs` uses the same
        // parser. The helper does NOT filter by existence — the
        // startup resolver additionally drops paths that do not exist
        // on disk (to keep the install log quiet) and deduplicates
        // against previously-resolved entries from earlier matchers.
        for (resolved, mode) in parse_file_changed_matcher(pattern, &cwd) {
            if !resolved.exists() {
                tracing::debug!(
                    path = %resolved.display(),
                    "FileChangedWatcher: matcher path does not exist, skipping"
                );
                continue;
            }

            if result.iter().any(|(p, _)| p == &resolved) {
                continue;
            }
            result.push((resolved, mode));
        }
    }

    result
}

#[async_trait::async_trait]
impl<
    A: Services + EnvironmentInfra<Config = forge_config::ForgeConfig>,
    F: CommandInfra
        + EnvironmentInfra<Config = forge_config::ForgeConfig>
        + SkillRepository
        + GrpcInfra,
> API for ForgeAPI<A, F>
{
    async fn discover(&self) -> Result<Vec<File>> {
        let environment = self.services.get_environment();
        let config = Walker::unlimited().cwd(environment.cwd);
        self.services.collect_files(config).await
    }

    async fn get_tools(&self) -> anyhow::Result<ToolsOverview> {
        self.app().list_tools().await
    }

    async fn get_models(&self) -> Result<Vec<Model>> {
        self.app().get_models().await
    }

    async fn get_all_provider_models(&self) -> Result<Vec<ProviderModels>> {
        self.app().get_all_provider_models().await
    }

    async fn get_agents(&self) -> Result<Vec<Agent>> {
        self.services.get_agents().await
    }

    async fn get_providers(&self) -> Result<Vec<AnyProvider>> {
        Ok(self.services.get_all_providers().await?)
    }

    async fn commit(
        &self,
        preview: bool,
        max_diff_size: Option<usize>,
        diff: Option<String>,
        additional_context: Option<String>,
    ) -> Result<forge_app::CommitResult> {
        let git_app = GitApp::new(self.services.clone());
        let result = git_app
            .commit_message(max_diff_size, diff, additional_context)
            .await?;

        if preview {
            Ok(result)
        } else {
            git_app
                .commit(result.message, result.has_staged_files)
                .await
        }
    }

    async fn get_provider(&self, id: &ProviderId) -> Result<AnyProvider> {
        let providers = self.services.get_all_providers().await?;
        Ok(providers
            .into_iter()
            .find(|p| p.id() == *id)
            .ok_or_else(|| Error::provider_not_available(id.clone()))?)
    }

    async fn chat(
        &self,
        chat: ChatRequest,
    ) -> anyhow::Result<MpscStream<Result<ChatResponse, anyhow::Error>>> {
        let agent_id = self
            .services
            .get_active_agent_id()
            .await?
            .unwrap_or_default();
        self.app().chat(agent_id, chat).await
    }

    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.services.upsert_conversation(conversation).await
    }

    async fn compact_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<CompactionResult> {
        let agent_id = self
            .services
            .get_active_agent_id()
            .await?
            .unwrap_or_default();
        self.app()
            .compact_conversation(agent_id, conversation_id)
            .await
    }

    fn environment(&self) -> Environment {
        self.services.get_environment().clone()
    }

    async fn conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>> {
        self.services.find_conversation(conversation_id).await
    }

    async fn get_conversations(&self, limit: Option<usize>) -> anyhow::Result<Vec<Conversation>> {
        Ok(self
            .services
            .get_conversations(limit)
            .await?
            .unwrap_or_default())
    }

    async fn last_conversation(&self) -> anyhow::Result<Option<Conversation>> {
        self.services.last_conversation().await
    }

    async fn delete_conversation(&self, conversation_id: &ConversationId) -> anyhow::Result<()> {
        self.services.delete_conversation(conversation_id).await
    }

    async fn rename_conversation(
        &self,
        conversation_id: &ConversationId,
        title: String,
    ) -> anyhow::Result<()> {
        self.services
            .modify_conversation(conversation_id, |conv| {
                conv.title = Some(title);
            })
            .await
    }

    async fn execute_shell_command(
        &self,
        command: &str,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        self.infra
            .execute_command(command.to_string(), working_dir, false, None, None)
            .await
    }
    async fn read_mcp_config(&self, scope: Option<&Scope>) -> Result<McpConfig> {
        self.services
            .read_mcp_config(scope)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn write_mcp_config(&self, scope: &Scope, config: &McpConfig) -> Result<()> {
        self.services
            .write_mcp_config(config, scope)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn execute_shell_command_raw(
        &self,
        command: &str,
    ) -> anyhow::Result<std::process::ExitStatus> {
        let cwd = self.environment().cwd;
        self.infra
            .execute_command_raw(command, cwd, None, None)
            .await
    }

    async fn get_agent_provider(&self, agent_id: AgentId) -> anyhow::Result<Provider<Url>> {
        let agent_provider_resolver = AgentProviderResolver::new(self.services.clone());
        agent_provider_resolver.get_provider(Some(agent_id)).await
    }

    async fn update_config(&self, ops: Vec<forge_domain::ConfigOperation>) -> anyhow::Result<()> {
        // Determine whether any op affects provider/model resolution before writing,
        // so we can invalidate the agent cache afterwards.
        let needs_agent_reload = ops
            .iter()
            .any(|op| matches!(op, forge_domain::ConfigOperation::SetSessionConfig(_)));
        let result = self.services.update_config(ops).await;
        if needs_agent_reload {
            let _ = self.services.reload_agents().await;
        }
        result
    }

    async fn get_commit_config(&self) -> anyhow::Result<Option<ModelConfig>> {
        self.services.get_commit_config().await
    }

    async fn get_suggest_config(&self) -> anyhow::Result<Option<ModelConfig>> {
        self.services.get_suggest_config().await
    }

    async fn get_reasoning_effort(&self) -> anyhow::Result<Option<Effort>> {
        self.services.get_reasoning_effort().await
    }

    async fn user_info(&self) -> Result<Option<User>> {
        let provider = self.get_default_provider().await?;
        if let Some(api_key) = provider.api_key() {
            let user_info = self.services.user_info(api_key.as_str()).await?;
            return Ok(Some(user_info));
        }
        Ok(None)
    }

    async fn user_usage(&self) -> Result<Option<UserUsage>> {
        let provider = self.get_default_provider().await?;
        if let Some(api_key) = provider
            .credential
            .as_ref()
            .and_then(|c| match &c.auth_details {
                forge_domain::AuthDetails::ApiKey(key) => Some(key.as_str()),
                _ => None,
            })
        {
            let user_usage = self.services.user_usage(api_key).await?;
            return Ok(Some(user_usage));
        }
        Ok(None)
    }

    async fn get_active_agent(&self) -> Option<AgentId> {
        self.services.get_active_agent_id().await.ok().flatten()
    }

    async fn set_active_agent(&self, agent_id: AgentId) -> anyhow::Result<()> {
        self.services.set_active_agent_id(agent_id).await
    }

    async fn get_agent_model(&self, agent_id: AgentId) -> Option<ModelId> {
        let agent_provider_resolver = AgentProviderResolver::new(self.services.clone());
        agent_provider_resolver.get_model(Some(agent_id)).await.ok()
    }

    async fn get_default_model(&self) -> Option<ModelId> {
        self.services.get_provider_model(None).await.ok()
    }

    async fn reload_mcp(&self) -> Result<()> {
        self.services.mcp_service().reload_mcp().await
    }
    async fn get_commands(&self) -> Result<Vec<Command>> {
        self.services.get_commands().await
    }

    async fn get_skills(&self) -> Result<Vec<Skill>> {
        self.infra.load_skills().await
    }

    async fn generate_command(&self, prompt: UserPrompt) -> Result<String> {
        use forge_app::CommandGenerator;
        let generator = CommandGenerator::new(self.services.clone());
        generator.generate(prompt).await
    }

    async fn init_provider_auth(
        &self,
        provider_id: ProviderId,
        method: AuthMethod,
    ) -> Result<AuthContextRequest> {
        Ok(self
            .services
            .init_provider_auth(provider_id, method)
            .await?)
    }

    async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        context: AuthContextResponse,
        timeout: Duration,
    ) -> Result<()> {
        Ok(self
            .services
            .complete_provider_auth(provider_id, context, timeout)
            .await?)
    }

    async fn remove_provider(&self, provider_id: &ProviderId) -> Result<()> {
        self.services.remove_credential(provider_id).await
    }

    async fn sync_workspace(
        &self,
        path: PathBuf,
    ) -> Result<MpscStream<Result<forge_domain::SyncProgress>>> {
        self.services.sync_workspace(path).await
    }

    async fn query_workspace(
        &self,
        path: PathBuf,
        params: forge_domain::SearchParams<'_>,
    ) -> Result<Vec<forge_domain::Node>> {
        self.services.query_workspace(path, params).await
    }

    async fn list_workspaces(&self) -> Result<Vec<forge_domain::WorkspaceInfo>> {
        self.services.list_workspaces().await
    }

    async fn get_workspace_info(
        &self,
        path: PathBuf,
    ) -> Result<Option<forge_domain::WorkspaceInfo>> {
        self.services.get_workspace_info(path).await
    }

    async fn delete_workspaces(&self, workspace_ids: Vec<forge_domain::WorkspaceId>) -> Result<()> {
        self.services.delete_workspaces(&workspace_ids).await
    }

    async fn get_workspace_status(&self, path: PathBuf) -> Result<Vec<forge_domain::FileStatus>> {
        self.services.get_workspace_status(path).await
    }

    async fn is_authenticated(&self) -> Result<bool> {
        self.services.is_authenticated().await
    }

    async fn create_auth_credentials(&self) -> Result<forge_domain::WorkspaceAuth> {
        self.services.init_auth_credentials().await
    }

    async fn init_workspace(&self, path: PathBuf) -> Result<forge_domain::WorkspaceId> {
        self.services.init_workspace(path).await
    }

    async fn migrate_env_credentials(&self) -> Result<Option<forge_domain::MigrationResult>> {
        Ok(self.services.migrate_env_credentials().await?)
    }

    async fn generate_data(
        &self,
        data_parameters: DataGenerationParameters,
    ) -> Result<BoxStream<'static, Result<serde_json::Value, anyhow::Error>>> {
        let app = DataGenerationApp::new(self.services.clone());
        app.execute(data_parameters).await
    }

    async fn get_default_provider(&self) -> Result<Provider<Url>> {
        let provider_id = self.services.get_default_provider().await?;
        self.services.get_provider(provider_id).await
    }

    async fn mcp_auth(&self, server_url: &str) -> Result<()> {
        let env = self.services.get_environment().clone();
        forge_infra::mcp_auth(server_url, &env).await
    }

    async fn mcp_logout(&self, server_url: Option<&str>) -> Result<()> {
        let env = self.services.get_environment().clone();
        match server_url {
            Some(url) => forge_infra::mcp_logout(url, &env).await,
            None => forge_infra::mcp_logout_all(&env).await,
        }
    }

    async fn mcp_auth_status(&self, server_url: &str) -> Result<String> {
        let env = self.services.get_environment().clone();
        Ok(forge_infra::mcp_auth_status(server_url, &env).await)
    }

    async fn list_plugins_with_errors(&self) -> Result<forge_domain::PluginLoadResult> {
        self.services.list_plugins_with_errors().await
    }

    async fn set_plugin_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        use std::collections::BTreeMap;

        use forge_config::PluginSetting;

        // Round-trip the persisted config through the reader/writer so
        // unrelated fields (session, providers, …) are preserved. The
        // in-memory services cache is refreshed via `reload_plugins` by
        // the calling slash command.
        let mut fc = ForgeConfig::read().unwrap_or_default();
        let entry = fc
            .plugins
            .get_or_insert_with(BTreeMap::new)
            .entry(name.to_string())
            .or_insert_with(|| PluginSetting { enabled: true, options: None });
        entry.enabled = enabled;

        // Wave C Part 2: mark this write as internal *before* the
        // actual `fc.write()?` so the `ConfigWatcher` debouncer
        // callback ignores the resulting filesystem event. Without
        // this suppression every `/plugin enable` / `/plugin disable`
        // would round-trip through the `ConfigChange` plugin hook
        // with `source: UserSettings`.
        let config_path = ConfigReader::config_path();
        self.mark_config_write(&config_path);

        fc.write()?;
        Ok(())
    }

    async fn reload_plugins(&self) -> Result<()> {
        self.services.reload_plugin_components().await
    }

    fn notification_service(&self) -> Arc<dyn NotificationService> {
        // `ForgeNotificationService` is cheap to construct — it holds only
        // an `Arc<S>` — so we construct a fresh instance per call instead
        // of caching on `ForgeAPI`. This also sidesteps a storage-side
        // circular-dependency problem where a cached
        // `ForgeNotificationService<ForgeServices<...>>` would have to
        // name the fully monomorphized services type at every callsite.
        Arc::new(ForgeNotificationService::new(self.services.clone()))
    }

    async fn fire_setup_hook(&self, trigger: SetupTrigger) -> Result<()> {
        fire_setup_hook(self.services.clone(), trigger).await
    }

    fn mark_config_write(&self, path: &Path) {
        if let Some(ref watcher) = self._config_watcher {
            watcher.mark_internal_write(path);
        }
    }

    fn hydrate_channel(&self) -> Result<()> {
        self.infra.hydrate();
        Ok(())
    }
}

impl<A: Send + Sync, F: ConsoleWriter> ConsoleWriter for ForgeAPI<A, F> {
    fn write(&self, buf: &[u8]) -> std::io::Result<usize> {
        self.infra.write(buf)
    }

    fn write_err(&self, buf: &[u8]) -> std::io::Result<usize> {
        self.infra.write_err(buf)
    }

    fn flush(&self) -> std::io::Result<()> {
        self.infra.flush()
    }

    fn flush_err(&self) -> std::io::Result<()> {
        self.infra.flush_err()
    }
}
