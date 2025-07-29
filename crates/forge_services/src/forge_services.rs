use std::sync::Arc;

use forge_app::Services;

use crate::app_config::ForgeConfigService;
use crate::attachment::ForgeChatRequest;
use crate::auth::ForgeAuthService;
use crate::conversation::ForgeConversationService;
use crate::discovery::ForgeDiscoveryService;
use crate::env::ForgeEnvironmentService;
use crate::infra::HttpInfra;
use crate::mcp::{ForgeMcpManager, ForgeMcpService};
use crate::provider::ForgeProviderService;
use crate::provider_registry::ForgeProviderRegistry;
use crate::template::ForgeTemplateService;
use crate::tool_services::{
    ForgeFetch, ForgeFollowup, ForgeFsCreate, ForgeFsPatch, ForgeFsRead, ForgeFsRemove,
    ForgeFsSearch, ForgeFsUndo, ForgeShell,
};
use crate::workflow::ForgeWorkflowService;
use crate::{
    CommandInfra, EnvironmentInfra, FileDirectoryInfra, FileInfoInfra, FileReaderInfra,
    FileRemoverInfra, FileWriterInfra, McpServerInfra, SnapshotInfra, UserInfra, WalkerInfra,
};

type McpService<F> = ForgeMcpService<ForgeMcpManager<F>, F, <F as McpServerInfra>::Client>;
type AuthService<F> = ForgeAuthService<F>;

/// ForgeApp is the main application container that implements the App trait.
/// It provides access to all core services required by the application.
///
/// Type Parameters:
/// - F: The infrastructure implementation that provides core services like
///   environment, file reading, vector indexing, and embedding.
#[derive(Clone)]
pub struct ForgeServices<F: HttpInfra + EnvironmentInfra + McpServerInfra + WalkerInfra> {
    chat_service: Arc<ForgeProviderService<F>>,
    conversation_service: Arc<ForgeConversationService<McpService<F>>>,
    template_service: Arc<ForgeTemplateService<F>>,
    attachment_service: Arc<ForgeChatRequest<F>>,
    workflow_service: Arc<ForgeWorkflowService<F>>,
    discovery_service: Arc<ForgeDiscoveryService<F>>,
    mcp_manager: Arc<ForgeMcpManager<F>>,
    file_create_service: Arc<ForgeFsCreate<F>>,
    file_read_service: Arc<ForgeFsRead<F>>,
    file_search_service: Arc<ForgeFsSearch<F>>,
    file_remove_service: Arc<ForgeFsRemove<F>>,
    file_patch_service: Arc<ForgeFsPatch<F>>,
    file_undo_service: Arc<ForgeFsUndo<F>>,
    shell_service: Arc<ForgeShell<F>>,
    fetch_service: Arc<ForgeFetch>,
    followup_service: Arc<ForgeFollowup<F>>,
    mcp_service: Arc<McpService<F>>,
    env_service: Arc<ForgeEnvironmentService<F>>,
    config_service: Arc<ForgeConfigService<F>>,
    auth_service: Arc<AuthService<F>>,
    provider_service: Arc<ForgeProviderRegistry<F>>,
}

impl<
    F: McpServerInfra
        + EnvironmentInfra
        + FileWriterInfra
        + FileInfoInfra
        + FileReaderInfra
        + HttpInfra
        + WalkerInfra,
> ForgeServices<F>
{
    pub fn new(infra: Arc<F>) -> Self {
        let mcp_manager = Arc::new(ForgeMcpManager::new(infra.clone()));
        let mcp_service = Arc::new(ForgeMcpService::new(mcp_manager.clone(), infra.clone()));
        let template_service = Arc::new(ForgeTemplateService::new(infra.clone()));
        let attachment_service = Arc::new(ForgeChatRequest::new(infra.clone()));

        let workflow_service = Arc::new(ForgeWorkflowService::new(infra.clone()));
        let suggestion_service = Arc::new(ForgeDiscoveryService::new(infra.clone()));
        let conversation_service = Arc::new(ForgeConversationService::new(mcp_service.clone()));
        let config_service = Arc::new(ForgeConfigService::new(infra.clone()));
        let auth_service = Arc::new(ForgeAuthService::new(infra.clone()));
        let chat_service = Arc::new(ForgeProviderService::<F>::new(infra.clone()));
        let file_create_service = Arc::new(ForgeFsCreate::new(infra.clone()));
        let file_read_service = Arc::new(ForgeFsRead::new(infra.clone()));
        let file_search_service = Arc::new(ForgeFsSearch::new(infra.clone()));
        let file_remove_service = Arc::new(ForgeFsRemove::new(infra.clone()));
        let file_patch_service = Arc::new(ForgeFsPatch::new(infra.clone()));
        let file_undo_service = Arc::new(ForgeFsUndo::new(infra.clone()));
        let shell_service = Arc::new(ForgeShell::new(infra.clone()));
        let fetch_service = Arc::new(ForgeFetch::new());
        let followup_service = Arc::new(ForgeFollowup::new(infra.clone()));
        let provider_service = Arc::new(ForgeProviderRegistry::new(infra.clone()));
        let env_service = Arc::new(ForgeEnvironmentService::new(infra));
        Self {
            conversation_service,
            attachment_service,
            template_service,
            workflow_service,
            discovery_service: suggestion_service,
            mcp_manager,
            file_create_service,
            file_read_service,
            file_search_service,
            file_remove_service,
            file_patch_service,
            file_undo_service,
            shell_service,
            fetch_service,
            followup_service,
            mcp_service,
            env_service,
            config_service,
            auth_service,
            chat_service,
            provider_service,
        }
    }
}

impl<
    F: FileReaderInfra
        + FileWriterInfra
        + CommandInfra
        + UserInfra
        + SnapshotInfra
        + McpServerInfra
        + FileRemoverInfra
        + FileInfoInfra
        + FileDirectoryInfra
        + EnvironmentInfra
        + HttpInfra
        + WalkerInfra
        + Clone,
> Services for ForgeServices<F>
{
    type ProviderService = ForgeProviderService<F>;
    type ConversationService = ForgeConversationService<McpService<F>>;
    type TemplateService = ForgeTemplateService<F>;
    type AttachmentService = ForgeChatRequest<F>;
    type EnvironmentService = ForgeEnvironmentService<F>;
    type WorkflowService = ForgeWorkflowService<F>;
    type FileDiscoveryService = ForgeDiscoveryService<F>;
    type McpConfigManager = ForgeMcpManager<F>;
    type FsCreateService = ForgeFsCreate<F>;
    type FsPatchService = ForgeFsPatch<F>;
    type FsReadService = ForgeFsRead<F>;
    type FsRemoveService = ForgeFsRemove<F>;
    type FsSearchService = ForgeFsSearch<F>;
    type FollowUpService = ForgeFollowup<F>;
    type FsUndoService = ForgeFsUndo<F>;
    type NetFetchService = ForgeFetch;
    type ShellService = ForgeShell<F>;
    type McpService = McpService<F>;
    type AppConfigService = ForgeConfigService<F>;
    type AuthService = AuthService<F>;
    type ProviderRegistry = ForgeProviderRegistry<F>;

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

    fn app_config_service(&self) -> &Self::AppConfigService {
        self.config_service.as_ref()
    }

    fn provider_registry(&self) -> &Self::ProviderRegistry {
        &self.provider_service
    }
}
