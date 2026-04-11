use std::sync::Arc;

use forge_app::{
    AgentRepository, AsyncHookResultQueue, CommandInfra, DirectoryReaderInfra, EnvironmentInfra,
    FileDirectoryInfra, FileInfoInfra, FileReaderInfra, FileRemoverInfra, FileWriterInfra,
    HttpInfra, KVStore, McpServerInfra, Services, SessionEnvCache, StrategyFactory, UserInfra,
    WalkerInfra,
};
use forge_domain::{
    ChatRepository, ConversationRepository, FuzzySearchRepository, LoadedPlugin, PluginLoadResult,
    PluginRepository, ProviderRepository, SkillRepository, SnapshotRepository,
    ValidationRepository, WorkspaceIndexRepository,
};

use crate::ForgeProviderAuthService;
use crate::agent_registry::ForgeAgentRegistryService;
use crate::app_config::ForgeAppConfigService;
use crate::attachment::ForgeChatRequest;
use crate::auth::ForgeAuthService;
use crate::command::CommandLoaderService as ForgeCommandLoaderService;
use crate::conversation::ForgeConversationService;
use crate::discovery::ForgeDiscoveryService;
use crate::elicitation_dispatcher::ForgeElicitationDispatcher;
use crate::fd::FdDefault;
use crate::hook_runtime::{ForgeHookConfigLoader, ForgeHookExecutor};
use crate::instructions::ForgeCustomInstructionsService;
use crate::mcp::{ForgeMcpManager, ForgeMcpService};
use crate::policy::ForgePolicyService;
use crate::provider_service::ForgeProviderService;
use crate::template::ForgeTemplateService;
use crate::tool_services::{
    ForgeFetch, ForgeFollowup, ForgeFsPatch, ForgeFsRead, ForgeFsRemove, ForgeFsSearch,
    ForgeFsUndo, ForgeFsWrite, ForgeImageRead, ForgePlanCreate, ForgePluginLoader, ForgeShell,
    ForgeSkillFetch,
};

type McpService<F> = ForgeMcpService<ForgeMcpManager<F>, F, <F as McpServerInfra>::Client>;
type AuthService<F> = ForgeAuthService<F>;

/// Type-erased adapter that turns any `Arc<F: PluginRepository>` into an
/// `Arc<dyn PluginRepository>`, so we can hand the plugin repository to
/// services (like `CommandLoaderService`) that store a trait object.
///
/// Kept private to `forge_services` because it exists solely to bridge
/// the generic infra into the dyn-object API used by downstream services.
struct InfraPluginRepository<F> {
    infra: Arc<F>,
}

#[async_trait::async_trait]
impl<F> PluginRepository for InfraPluginRepository<F>
where
    F: PluginRepository + Send + Sync + 'static,
{
    async fn load_plugins(&self) -> anyhow::Result<Vec<LoadedPlugin>> {
        self.infra.load_plugins().await
    }

    async fn load_plugins_with_errors(&self) -> anyhow::Result<PluginLoadResult> {
        self.infra.load_plugins_with_errors().await
    }
}

/// ForgeApp is the main application container that implements the App trait.
/// It provides access to all core services required by the application.
///
/// Type Parameters:
/// - F: The infrastructure implementation that provides core services like
///   environment, file reading, vector indexing, and embedding.
/// - R: The repository implementation that provides data persistence
#[derive(Clone)]
pub struct ForgeServices<
    F: HttpInfra
        + EnvironmentInfra
        + McpServerInfra
        + WalkerInfra
        + SnapshotRepository
        + ConversationRepository
        + KVStore
        + ChatRepository
        + ProviderRepository
        + WorkspaceIndexRepository
        + AgentRepository
        + SkillRepository
        + ValidationRepository,
> {
    chat_service: Arc<ForgeProviderService<F>>,
    config_service: Arc<ForgeAppConfigService<F>>,
    conversation_service: Arc<ForgeConversationService<F>>,
    template_service: Arc<ForgeTemplateService<F>>,
    attachment_service: Arc<ForgeChatRequest<F>>,
    discovery_service: Arc<ForgeDiscoveryService<F>>,
    mcp_manager: Arc<ForgeMcpManager<F>>,
    file_create_service: Arc<ForgeFsWrite<F>>,
    plan_create_service: Arc<ForgePlanCreate<F>>,
    file_read_service: Arc<ForgeFsRead<F>>,
    image_read_service: Arc<ForgeImageRead<F>>,
    file_search_service: Arc<ForgeFsSearch<F>>,
    file_remove_service: Arc<ForgeFsRemove<F>>,
    file_patch_service: Arc<ForgeFsPatch<F>>,
    file_undo_service: Arc<ForgeFsUndo<F>>,
    shell_service: Arc<ForgeShell<F>>,
    fetch_service: Arc<ForgeFetch>,
    followup_service: Arc<ForgeFollowup<F>>,
    mcp_service: Arc<McpService<F>>,
    custom_instructions_service: Arc<ForgeCustomInstructionsService<F>>,
    auth_service: Arc<AuthService<F>>,
    agent_registry_service: Arc<ForgeAgentRegistryService<F>>,
    command_loader_service: Arc<ForgeCommandLoaderService<F>>,
    policy_service: ForgePolicyService<F>,
    provider_auth_service: ForgeProviderAuthService<F>,
    workspace_service: Arc<crate::context_engine::ForgeWorkspaceService<F, FdDefault<F>>>,
    skill_service: Arc<ForgeSkillFetch<F>>,
    plugin_loader_service: Arc<ForgePluginLoader<F>>,
    hook_config_loader_service: Arc<ForgeHookConfigLoader<F>>,
    hook_executor_service: Arc<ForgeHookExecutor<F>>,
    /// Shared queue for async-rewake hook results. Populated by the
    /// shell executor's background tasks; drained by the orchestrator
    /// between conversation turns.
    async_hook_queue: AsyncHookResultQueue,
    session_env_cache: SessionEnvCache,
    /// Phase 8 elicitation dispatcher. Owns a `OnceLock<Arc<Self>>`
    /// populated after construction via
    /// [`ForgeServices::init_elicitation_dispatcher`]; see the
    /// module-level doc on [`ForgeElicitationDispatcher`] for the
    /// cycle rationale.
    elicitation_dispatcher: Arc<ForgeElicitationDispatcher<ForgeServices<F>>>,
    infra: Arc<F>,
}

impl<
    F: McpServerInfra
        + EnvironmentInfra<Config = forge_config::ForgeConfig>
        + FileWriterInfra
        + FileInfoInfra
        + FileReaderInfra
        + HttpInfra
        + WalkerInfra
        + DirectoryReaderInfra
        + CommandInfra
        + UserInfra
        + SnapshotRepository
        + ConversationRepository
        + ChatRepository
        + ProviderRepository
        + KVStore
        + WorkspaceIndexRepository
        + AgentRepository
        + SkillRepository
        + PluginRepository
        + ValidationRepository
        + Send
        + Sync
        + 'static,
> ForgeServices<F>
{
    pub fn new(infra: Arc<F>) -> Self {
        // Plugin-aware MCP manager: plugin-contributed servers are merged
        // into `read_mcp_config` output under the `"{plugin}:{server}"`
        // namespace. Uses the same dyn-object adapter as the command /
        // hook loaders so all three subsystems share one view of disk
        // scans without coupling to the concrete infra type.
        let mcp_plugin_repo: Arc<dyn PluginRepository> =
            Arc::new(InfraPluginRepository { infra: infra.clone() });
        let mcp_manager = Arc::new(ForgeMcpManager::with_plugin_repository(
            infra.clone(),
            mcp_plugin_repo,
        ));
        let mcp_service = Arc::new(ForgeMcpService::new(mcp_manager.clone(), infra.clone()));
        let template_service = Arc::new(ForgeTemplateService::new(infra.clone()));
        let attachment_service = Arc::new(ForgeChatRequest::new(infra.clone()));
        let suggestion_service = Arc::new(ForgeDiscoveryService::new(infra.clone()));
        let conversation_service = Arc::new(ForgeConversationService::new(infra.clone()));
        let auth_service = Arc::new(ForgeAuthService::new(infra.clone()));
        let chat_service = Arc::new(ForgeProviderService::new(infra.clone()));
        let config_service = Arc::new(ForgeAppConfigService::new(infra.clone()));
        let file_create_service = Arc::new(ForgeFsWrite::new(infra.clone()));
        let plan_create_service = Arc::new(ForgePlanCreate::new(infra.clone()));
        let file_read_service = Arc::new(ForgeFsRead::new(infra.clone()));
        let image_read_service = Arc::new(ForgeImageRead::new(infra.clone()));
        let file_search_service = Arc::new(ForgeFsSearch::new(infra.clone()));
        let file_remove_service = Arc::new(ForgeFsRemove::new(infra.clone()));
        let file_patch_service = Arc::new(ForgeFsPatch::new(infra.clone()));
        let file_undo_service = Arc::new(ForgeFsUndo::new(infra.clone()));
        let session_env_cache = SessionEnvCache::new();
        let shell_service = Arc::new(ForgeShell::new(infra.clone(), session_env_cache.clone()));
        let fetch_service = Arc::new(ForgeFetch::new());
        let followup_service = Arc::new(ForgeFollowup::new(infra.clone()));
        let custom_instructions_service =
            Arc::new(ForgeCustomInstructionsService::new(infra.clone()));
        let agent_registry_service = Arc::new(ForgeAgentRegistryService::new(infra.clone()));
        let plugin_repository_dyn: Arc<dyn PluginRepository> =
            Arc::new(InfraPluginRepository { infra: infra.clone() });
        let command_loader_service = Arc::new(ForgeCommandLoaderService::new(
            infra.clone(),
            plugin_repository_dyn,
        ));
        let policy_service = ForgePolicyService::new(infra.clone());
        let provider_auth_service = ForgeProviderAuthService::new(infra.clone());
        let discovery = Arc::new(FdDefault::new(infra.clone()));
        let workspace_service = Arc::new(crate::context_engine::ForgeWorkspaceService::new(
            infra.clone(),
            discovery,
        ));
        let skill_service = Arc::new(ForgeSkillFetch::new(infra.clone()));
        let plugin_loader_service = Arc::new(ForgePluginLoader::new(infra.clone()));
        // Hook runtime: reuse the same dyn-object plugin repository adapter as
        // the command loader so the loader caches disk scans independently from
        // the command-level cache.
        let hook_plugin_repo: Arc<dyn PluginRepository> =
            Arc::new(InfraPluginRepository { infra: infra.clone() });
        let hook_config_loader_service =
            Arc::new(ForgeHookConfigLoader::new(infra.clone(), hook_plugin_repo));

        // Create the async-rewake channel + queue. The sender goes into
        // the hook executor; the receiver feeds a background pump that
        // pushes results into the shared queue.
        let async_hook_queue = AsyncHookResultQueue::new();
        let (async_result_tx, mut async_result_rx) =
            tokio::sync::mpsc::unbounded_channel::<forge_domain::PendingHookResult>();
        {
            let queue = async_hook_queue.clone();
            tokio::spawn(async move {
                while let Some(result) = async_result_rx.recv().await {
                    queue.push(result).await;
                }
            });
        }
        let hook_executor_service =
            Arc::new(ForgeHookExecutor::new(infra.clone()).with_async_result_tx(async_result_tx));

        // Phase 8 elicitation dispatcher. Created with an empty
        // services slot; populated by
        // `init_elicitation_dispatcher` once `Arc<ForgeServices<F>>`
        // exists. See the module-level doc on
        // `ForgeElicitationDispatcher` for the cycle rationale.
        let elicitation_dispatcher = Arc::new(ForgeElicitationDispatcher::new());

        Self {
            conversation_service,
            attachment_service,
            template_service,
            discovery_service: suggestion_service,
            mcp_manager,
            file_create_service,
            plan_create_service,
            file_read_service,
            image_read_service,
            file_search_service,
            file_remove_service,
            file_patch_service,
            file_undo_service,
            shell_service,
            fetch_service,
            followup_service,
            mcp_service,
            custom_instructions_service,
            auth_service,
            config_service,
            agent_registry_service,
            command_loader_service,
            policy_service,
            provider_auth_service,
            workspace_service,
            skill_service,
            plugin_loader_service,
            hook_config_loader_service,
            hook_executor_service,
            async_hook_queue,
            session_env_cache,
            elicitation_dispatcher,
            chat_service,
            infra,
        }
    }

    /// Populate the elicitation dispatcher's services slot. Must be
    /// called from the `forge_api` layer immediately after
    /// `Arc::new(ForgeServices::new(...))` returns so the dispatcher
    /// can fire hooks against the fully-constructed aggregate. First
    /// call wins; subsequent calls are silent no-ops per the
    /// underlying [`std::sync::OnceLock`] contract.
    ///
    /// Until this method runs the dispatcher declines every request
    /// with a warn log — see
    /// [`ForgeElicitationDispatcher::elicit`].
    pub fn init_elicitation_dispatcher(self: &Arc<Self>) {
        self.elicitation_dispatcher.init(self.clone());
    }

    /// Populate the hook executor's LLM service handle. Must be called
    /// from the `forge_api` layer immediately after
    /// `Arc::new(ForgeServices::new(...))` returns — same timing as
    /// `init_elicitation_dispatcher`.
    ///
    /// Until this method runs, prompt and agent hooks return an error
    /// instead of making LLM calls.
    pub fn init_hook_executor_services(self: &Arc<Self>)
    where
        ForgeServices<F>: forge_app::Services,
    {
        self.hook_executor_service.init_services(
            self.clone() as std::sync::Arc<dyn crate::hook_runtime::executor::HookModelService>
        );
    }

    /// Return a type-erased handle to the elicitation dispatcher so
    /// it can be plumbed into [`forge_infra::ForgeInfra`] (which
    /// doesn't know the concrete `ForgeServices<F>` type — and
    /// shouldn't, to keep the `forge_infra` → `forge_app` dep graph
    /// flowing in one direction).
    ///
    /// Wave F-2: used by `forge_api::ForgeAPI::init` to hand the
    /// dispatcher to `ForgeMcpServer` via
    /// `ForgeInfra::init_elicitation_dispatcher`, closing the loop
    /// between the MCP client handler (which lives in `forge_infra`)
    /// and the hook-fire pipeline (which lives in `forge_services`).
    pub fn elicitation_dispatcher_arc(&self) -> Arc<dyn forge_app::ElicitationDispatcher>
    where
        ForgeServices<F>: forge_app::Services,
    {
        self.elicitation_dispatcher.clone()
    }

    /// Return a reference to the session env cache so the hook handler
    /// can later share it.
    pub fn session_env_cache(&self) -> &SessionEnvCache {
        &self.session_env_cache
    }
}

impl<
    F: FileReaderInfra
        + FileWriterInfra
        + CommandInfra
        + UserInfra
        + McpServerInfra
        + FileRemoverInfra
        + FileInfoInfra
        + FileDirectoryInfra
        + EnvironmentInfra<Config = forge_config::ForgeConfig>
        + DirectoryReaderInfra
        + HttpInfra
        + WalkerInfra
        + Clone
        + SnapshotRepository
        + ConversationRepository
        + KVStore
        + ChatRepository
        + ProviderRepository
        + AgentRepository
        + SkillRepository
        + PluginRepository
        + StrategyFactory
        + WorkspaceIndexRepository
        + ValidationRepository
        + FuzzySearchRepository
        + Clone
        + 'static,
> Services for ForgeServices<F>
{
    type AppConfigService = ForgeAppConfigService<F>;
    type ConversationService = ForgeConversationService<F>;
    type TemplateService = ForgeTemplateService<F>;
    type ProviderAuthService = ForgeProviderAuthService<F>;

    fn provider_auth_service(&self) -> &Self::ProviderAuthService {
        &self.provider_auth_service
    }
    type AttachmentService = ForgeChatRequest<F>;
    type CustomInstructionsService = ForgeCustomInstructionsService<F>;
    type FileDiscoveryService = ForgeDiscoveryService<F>;
    type McpConfigManager = ForgeMcpManager<F>;
    type FsWriteService = ForgeFsWrite<F>;
    type PlanCreateService = ForgePlanCreate<F>;
    type FsPatchService = ForgeFsPatch<F>;
    type FsReadService = ForgeFsRead<F>;
    type ImageReadService = ForgeImageRead<F>;
    type FsRemoveService = ForgeFsRemove<F>;
    type FsSearchService = ForgeFsSearch<F>;
    type FollowUpService = ForgeFollowup<F>;
    type FsUndoService = ForgeFsUndo<F>;
    type NetFetchService = ForgeFetch;
    type ShellService = ForgeShell<F>;
    type McpService = McpService<F>;
    type AuthService = AuthService<F>;
    type AgentRegistry = ForgeAgentRegistryService<F>;
    type CommandLoaderService = ForgeCommandLoaderService<F>;
    type PolicyService = ForgePolicyService<F>;
    type ProviderService = ForgeProviderService<F>;
    type WorkspaceService = crate::context_engine::ForgeWorkspaceService<F, FdDefault<F>>;
    type SkillFetchService = ForgeSkillFetch<F>;
    type PluginLoader = ForgePluginLoader<F>;
    type HookConfigLoader = ForgeHookConfigLoader<F>;
    type HookExecutor = ForgeHookExecutor<F>;
    type ElicitationDispatcher = ForgeElicitationDispatcher<ForgeServices<F>>;

    fn config_service(&self) -> &Self::AppConfigService {
        &self.config_service
    }

    fn conversation_service(&self) -> &Self::ConversationService {
        &self.conversation_service
    }

    fn template_service(&self) -> &Self::TemplateService {
        &self.template_service
    }

    fn attachment_service(&self) -> &Self::AttachmentService {
        &self.attachment_service
    }

    fn custom_instructions_service(&self) -> &Self::CustomInstructionsService {
        &self.custom_instructions_service
    }

    fn file_discovery_service(&self) -> &Self::FileDiscoveryService {
        self.discovery_service.as_ref()
    }

    fn mcp_config_manager(&self) -> &Self::McpConfigManager {
        self.mcp_manager.as_ref()
    }

    fn fs_create_service(&self) -> &Self::FsWriteService {
        &self.file_create_service
    }

    fn plan_create_service(&self) -> &Self::PlanCreateService {
        &self.plan_create_service
    }

    fn fs_patch_service(&self) -> &Self::FsPatchService {
        &self.file_patch_service
    }

    fn fs_read_service(&self) -> &Self::FsReadService {
        &self.file_read_service
    }

    fn fs_remove_service(&self) -> &Self::FsRemoveService {
        &self.file_remove_service
    }

    fn fs_search_service(&self) -> &Self::FsSearchService {
        &self.file_search_service
    }

    fn follow_up_service(&self) -> &Self::FollowUpService {
        &self.followup_service
    }

    fn fs_undo_service(&self) -> &Self::FsUndoService {
        &self.file_undo_service
    }

    fn net_fetch_service(&self) -> &Self::NetFetchService {
        &self.fetch_service
    }

    fn shell_service(&self) -> &Self::ShellService {
        &self.shell_service
    }

    fn mcp_service(&self) -> &Self::McpService {
        &self.mcp_service
    }

    fn auth_service(&self) -> &Self::AuthService {
        self.auth_service.as_ref()
    }

    fn agent_registry(&self) -> &Self::AgentRegistry {
        &self.agent_registry_service
    }

    fn command_loader_service(&self) -> &Self::CommandLoaderService {
        &self.command_loader_service
    }

    fn policy_service(&self) -> &Self::PolicyService {
        &self.policy_service
    }

    fn workspace_service(&self) -> &Self::WorkspaceService {
        &self.workspace_service
    }

    fn image_read_service(&self) -> &Self::ImageReadService {
        &self.image_read_service
    }
    fn skill_fetch_service(&self) -> &Self::SkillFetchService {
        &self.skill_service
    }

    fn plugin_loader(&self) -> &Self::PluginLoader {
        &self.plugin_loader_service
    }

    fn hook_config_loader(&self) -> &Self::HookConfigLoader {
        &self.hook_config_loader_service
    }

    fn hook_executor(&self) -> &Self::HookExecutor {
        &self.hook_executor_service
    }

    fn elicitation_dispatcher(&self) -> &Self::ElicitationDispatcher {
        &self.elicitation_dispatcher
    }

    fn async_hook_queue(&self) -> Option<&AsyncHookResultQueue> {
        Some(&self.async_hook_queue)
    }

    fn provider_service(&self) -> &Self::ProviderService {
        &self.chat_service
    }
}

impl<
    F: EnvironmentInfra<Config = forge_config::ForgeConfig>
        + HttpInfra
        + McpServerInfra
        + WalkerInfra
        + SnapshotRepository
        + ConversationRepository
        + KVStore
        + ChatRepository
        + ProviderRepository
        + WorkspaceIndexRepository
        + AgentRepository
        + SkillRepository
        + ValidationRepository
        + Send
        + Sync,
> forge_app::EnvironmentInfra for ForgeServices<F>
{
    type Config = forge_config::ForgeConfig;

    fn get_environment(&self) -> forge_domain::Environment {
        self.infra.get_environment()
    }

    fn get_config(&self) -> anyhow::Result<forge_config::ForgeConfig> {
        self.infra.get_config()
    }

    fn update_environment(
        &self,
        ops: Vec<forge_domain::ConfigOperation>,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        self.infra.update_environment(ops)
    }

    fn get_env_var(&self, key: &str) -> Option<String> {
        self.infra.get_env_var(key)
    }

    fn get_env_vars(&self) -> std::collections::BTreeMap<String, String> {
        self.infra.get_env_vars()
    }
}
