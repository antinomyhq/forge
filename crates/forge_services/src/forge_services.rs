use std::sync::Arc;

use forge_app::{
    AppConfigRepository, CommandInfra, DirectoryReaderInfra, EnvironmentInfra, FileDirectoryInfra,
    FileInfoInfra, FileReaderInfra, FileRemoverInfra, FileWriterInfra, HttpInfra, McpServerInfra,
    Services, UserInfra, WalkerInfra,
};
use forge_domain::{
    CacheRepository, ConversationRepository, ProviderRepository, SnapshotRepository,
};

use crate::agent_registry::AgentLoaderService as ForgeAgentLoaderService;
use crate::attachment::ForgeChatRequest;
use crate::auth::ForgeAuthService;
use crate::command_loader::CommandLoaderService as ForgeCommandLoaderService;
use crate::conversation::ForgeConversationService;
use crate::custom_instructions::ForgeCustomInstructionsService;
use crate::discovery::ForgeDiscoveryService;
use crate::env::ForgeEnvironmentService;
use crate::mcp::{ForgeMcpManager, ForgeMcpService};
use crate::policy::ForgePolicyService;
use crate::provider::{ForgeProviderRegistry, ForgeProviderService};
use crate::template::ForgeTemplateService;
use crate::tool_services::{
    ForgeFetch, ForgeFollowup, ForgeFsCreate, ForgeFsPatch, ForgeFsRead, ForgeFsRemove,
    ForgeFsSearch, ForgeFsUndo, ForgeImageRead, ForgePlanCreate, ForgeShell,
};
use crate::workflow::ForgeWorkflowService;

type McpService<F> = ForgeMcpService<ForgeMcpManager<F>, F, <F as McpServerInfra>::Client>;
type AuthService<F> = ForgeAuthService<F>;

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
        + AppConfigRepository
        + CacheRepository
        + ProviderRepository,
> {
    chat_service: Arc<ForgeProviderService<F>>,
    conversation_service: Arc<ForgeConversationService<F>>,
    template_service: Arc<ForgeTemplateService<F>>,
    attachment_service: Arc<ForgeChatRequest<F>>,
    workflow_service: Arc<ForgeWorkflowService<F>>,
    discovery_service: Arc<ForgeDiscoveryService<F>>,
    mcp_manager: Arc<ForgeMcpManager<F>>,
    file_create_service: Arc<ForgeFsCreate<F>>,
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
    env_service: Arc<ForgeEnvironmentService<F>>,
    custom_instructions_service: Arc<ForgeCustomInstructionsService<F>>,
    auth_service: Arc<AuthService<F>>,
    agent_loader_service: Arc<ForgeAgentLoaderService<F>>,
    command_loader_service: Arc<ForgeCommandLoaderService<F>>,
    policy_service: ForgePolicyService<F>,
    provider_registry: Arc<ForgeProviderRegistry<F>>,
}

impl<
    F: McpServerInfra
        + EnvironmentInfra
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
        + AppConfigRepository
        + CacheRepository
        + ProviderRepository,
> ForgeServices<F>
{
    pub fn new(infra: Arc<F>) -> Self {
        let mcp_manager = Arc::new(ForgeMcpManager::new(infra.clone()));
        let mcp_service = Arc::new(ForgeMcpService::new(mcp_manager.clone(), infra.clone()));
        let template_service = Arc::new(ForgeTemplateService::new(infra.clone()));
        let attachment_service = Arc::new(ForgeChatRequest::new(infra.clone()));
        let workflow_service = Arc::new(ForgeWorkflowService::new(infra.clone()));
        let suggestion_service = Arc::new(ForgeDiscoveryService::new(infra.clone()));
        let conversation_service = Arc::new(ForgeConversationService::new(infra.clone()));
        let auth_service = Arc::new(ForgeAuthService::new(infra.clone()));
        let chat_service = Arc::new(ForgeProviderService::new(infra.clone()));
        let file_create_service = Arc::new(ForgeFsCreate::new(infra.clone()));
        let plan_create_service = Arc::new(ForgePlanCreate::new(infra.clone()));
        let file_read_service = Arc::new(ForgeFsRead::new(infra.clone()));
        let image_read_service = Arc::new(ForgeImageRead::new(infra.clone()));
        let file_search_service = Arc::new(ForgeFsSearch::new(infra.clone()));
        let file_remove_service = Arc::new(ForgeFsRemove::new(infra.clone()));
        let file_patch_service = Arc::new(ForgeFsPatch::new(infra.clone()));
        let file_undo_service = Arc::new(ForgeFsUndo::new(infra.clone()));
        let shell_service = Arc::new(ForgeShell::new(infra.clone()));
        let fetch_service = Arc::new(ForgeFetch::new());
        let followup_service = Arc::new(ForgeFollowup::new(infra.clone()));
        let env_service = Arc::new(ForgeEnvironmentService::new(infra.clone()));
        let custom_instructions_service =
            Arc::new(ForgeCustomInstructionsService::new(infra.clone()));
        let agent_loader_service = Arc::new(ForgeAgentLoaderService::new(infra.clone()));
        let command_loader_service = Arc::new(ForgeCommandLoaderService::new(infra.clone()));
        let policy_service = ForgePolicyService::new(infra.clone());
        let provider_registry = Arc::new(ForgeProviderRegistry::new(infra.clone()));

        Self {
            conversation_service,
            attachment_service,
            template_service,
            workflow_service,
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
            env_service,
            custom_instructions_service,
            auth_service,
            chat_service,
            agent_loader_service,
            command_loader_service,
            policy_service,
            provider_registry,
        }
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
        + EnvironmentInfra
        + DirectoryReaderInfra
        + HttpInfra
        + WalkerInfra
        + Clone
        + SnapshotRepository
        + ConversationRepository
        + AppConfigRepository
        + CacheRepository
        + ProviderRepository
        + Clone
        + 'static,
> Services for ForgeServices<F>
{
    type ProviderService = ForgeProviderService<F>;
    type ConversationService = ForgeConversationService<F>;
    type TemplateService = ForgeTemplateService<F>;
    type AttachmentService = ForgeChatRequest<F>;
    type EnvironmentService = ForgeEnvironmentService<F>;
    type CustomInstructionsService = ForgeCustomInstructionsService<F>;
    type WorkflowService = ForgeWorkflowService<F>;
    type FileDiscoveryService = ForgeDiscoveryService<F>;
    type McpConfigManager = ForgeMcpManager<F>;
    type FsCreateService = ForgeFsCreate<F>;
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
    type AgentRegistry = ForgeAgentLoaderService<F>;
    type CommandLoaderService = ForgeCommandLoaderService<F>;
    type PolicyService = ForgePolicyService<F>;
    type ProviderRepository = ForgeProviderRegistry<F>;

    fn provider_service(&self) -> &Self::ProviderService {
        &self.chat_service
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

    fn environment_service(&self) -> &Self::EnvironmentService {
        &self.env_service
    }
    fn custom_instructions_service(&self) -> &Self::CustomInstructionsService {
        &self.custom_instructions_service
    }

    fn workflow_service(&self) -> &Self::WorkflowService {
        self.workflow_service.as_ref()
    }

    fn file_discovery_service(&self) -> &Self::FileDiscoveryService {
        self.discovery_service.as_ref()
    }

    fn mcp_config_manager(&self) -> &Self::McpConfigManager {
        self.mcp_manager.as_ref()
    }

    fn fs_create_service(&self) -> &Self::FsCreateService {
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
        &self.agent_loader_service
    }

    fn command_loader_service(&self) -> &Self::CommandLoaderService {
        &self.command_loader_service
    }

    fn policy_service(&self) -> &Self::PolicyService {
        &self.policy_service
    }
    fn image_read_service(&self) -> &Self::ImageReadService {
        &self.image_read_service
    }
}

#[async_trait::async_trait]
impl<
    F: McpServerInfra
        + EnvironmentInfra
        + FileWriterInfra
        + FileInfoInfra
        + FileReaderInfra
        + HttpInfra
        + WalkerInfra
        + DirectoryReaderInfra
        + CommandInfra
        + UserInfra
        + FileDirectoryInfra
        + FileRemoverInfra
        + SnapshotRepository
        + ConversationRepository
        + AppConfigRepository
        + CacheRepository
        + ProviderRepository,
> forge_domain::ProviderRepository for ForgeServices<F>
{
    async fn get_default_provider(&self) -> anyhow::Result<forge_domain::Provider> {
        self.provider_registry.get_default_provider().await
    }

    async fn set_default_provider(
        &self,
        provider_id: forge_domain::ProviderId,
    ) -> anyhow::Result<()> {
        self.provider_registry
            .set_default_provider(provider_id)
            .await
    }

    async fn get_all_providers(&self) -> anyhow::Result<Vec<forge_domain::Provider>> {
        self.provider_registry.get_all_providers().await
    }

    async fn get_default_model(
        &self,
        provider_id: &forge_domain::ProviderId,
    ) -> anyhow::Result<forge_domain::ModelId> {
        self.provider_registry.get_default_model(provider_id).await
    }

    async fn set_default_model(
        &self,
        model: forge_domain::ModelId,
        provider_id: forge_domain::ProviderId,
    ) -> anyhow::Result<()> {
        self.provider_registry
            .set_default_model(model, provider_id)
            .await
    }

    async fn get_provider(
        &self,
        id: forge_domain::ProviderId,
    ) -> anyhow::Result<forge_domain::Provider> {
        self.provider_registry.get_provider(id).await
    }
}
