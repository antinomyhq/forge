use std::sync::Arc;

use forge_app::Services;

use crate::attachment::ForgeChatRequest;
use crate::conversation::ForgeConversationService;
use crate::discovery::ForgeDiscoveryService;
use crate::mcp::{ForgeMcpManager, ForgeMcpService};
use crate::provider::ForgeProviderService;
use crate::template::ForgeTemplateService;
use crate::tool_service::ForgeToolService;
use crate::tools_v2::{
    ForgeFetch, ForgeFollowup, ForgeFsCreate, ForgeFsPatch, ForgeFsRead, ForgeFsRemove,
    ForgeFsSearch, ForgeFsUndo, ForgeShell,
};
use crate::workflow::ForgeWorkflowService;
use crate::{Infrastructure, McpServer};

type McpService<F> =
    ForgeMcpService<ForgeMcpManager<F>, F, <<F as Infrastructure>::McpServer as McpServer>::Client>;

/// ForgeApp is the main application container that implements the App trait.
/// It provides access to all core services required by the application.
///
/// Type Parameters:
/// - F: The infrastructure implementation that provides core services like
///   environment, file reading, vector indexing, and embedding.
#[derive(Clone)]
pub struct ForgeServices<F: Infrastructure> {
    infra: Arc<F>,
    tool_service: Arc<ForgeToolService<McpService<F>>>,
    provider_service: Arc<ForgeProviderService>,
    conversation_service: Arc<ForgeConversationService<McpService<F>>>,
    template_service: Arc<ForgeTemplateService>,
    attachment_service: Arc<ForgeChatRequest<F>>,
    workflow_service: Arc<ForgeWorkflowService<F>>,
    discovery_service: Arc<ForgeDiscoveryService<F>>,
    mcp_manager: Arc<ForgeMcpManager<F>>,
    file_create_service: Arc<ForgeFsCreate<F>>,
    file_read_service: Arc<ForgeFsRead<F>>,
    file_search_service: Arc<ForgeFsSearch>,
    file_remove_service: Arc<ForgeFsRemove<F>>,
    file_patch_service: Arc<ForgeFsPatch<F>>,
    file_undo_service: Arc<ForgeFsUndo<F>>,
    shell_service: Arc<ForgeShell<F>>,
    fetch_service: Arc<ForgeFetch>,
    followup_service: Arc<ForgeFollowup<F>>,
    mcp_service: Arc<McpService<F>>,
}

impl<F: Infrastructure> ForgeServices<F> {
    pub fn new(infra: Arc<F>) -> Self {
        let mcp_manager = Arc::new(ForgeMcpManager::new(infra.clone()));
        let mcp_service = Arc::new(ForgeMcpService::new(mcp_manager.clone(), infra.clone()));
        let tool_service = Arc::new(ForgeToolService::new(infra.clone(), mcp_service.clone()));
        let template_service = Arc::new(ForgeTemplateService::new());
        let provider_service = Arc::new(ForgeProviderService::new(infra.clone()));
        let attachment_service = Arc::new(ForgeChatRequest::new(infra.clone()));

        let conversation_service = Arc::new(ForgeConversationService::new(mcp_service.clone()));

        let workflow_service = Arc::new(ForgeWorkflowService::new(infra.clone()));
        let suggestion_service = Arc::new(ForgeDiscoveryService::new(infra.clone()));
        let file_create_service = Arc::new(ForgeFsCreate::new(infra.clone()));
        let file_read_service = Arc::new(ForgeFsRead::new(infra.clone()));
        let file_search_service = Arc::new(ForgeFsSearch::new());
        let file_remove_service = Arc::new(ForgeFsRemove::new(infra.clone()));
        let file_patch_service = Arc::new(ForgeFsPatch::new(infra.clone()));
        let file_undo_service = Arc::new(ForgeFsUndo::new(infra.clone()));
        let shell_service = Arc::new(ForgeShell::new(infra.clone()));
        let fetch_service = Arc::new(ForgeFetch::new());
        let followup_service = Arc::new(ForgeFollowup::new(infra.clone()));
        Self {
            infra,
            conversation_service,
            tool_service,
            attachment_service,
            provider_service,
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
        }
    }
}

impl<F: Infrastructure> Services for ForgeServices<F> {
    type ToolService = ForgeToolService<McpService<F>>;
    type ProviderService = ForgeProviderService;
    type ConversationService = ForgeConversationService<McpService<F>>;
    type TemplateService = ForgeTemplateService;
    type AttachmentService = ForgeChatRequest<F>;
    type EnvironmentService = F::EnvironmentService;
    type WorkflowService = ForgeWorkflowService<F>;
    type FileDiscoveryService = ForgeDiscoveryService<F>;
    type McpConfigManager = ForgeMcpManager<F>;
    type FsCreateService = ForgeFsCreate<F>;
    type FsPatchService = ForgeFsPatch<F>;
    type FsReadService = ForgeFsRead<F>;
    type FsRemoveService = ForgeFsRemove<F>;
    type FsSearchService = ForgeFsSearch;
    type FollowUpService = ForgeFollowup<F>;
    type FsUndoService = ForgeFsUndo<F>;
    type NetFetchService = ForgeFetch;
    type ShellService = ForgeShell<F>;
    type McpService = McpService<F>;

    fn tool_service(&self) -> &Self::ToolService {
        &self.tool_service
    }

    fn provider_service(&self) -> &Self::ProviderService {
        &self.provider_service
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
        self.infra.environment_service()
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
}

impl<F: Infrastructure> Infrastructure for ForgeServices<F> {
    type EnvironmentService = F::EnvironmentService;
    type FsReadService = F::FsReadService;
    type FsWriteService = F::FsWriteService;
    type FsMetaService = F::FsMetaService;
    type FsSnapshotService = F::FsSnapshotService;
    type FsRemoveService = F::FsRemoveService;
    type FsCreateDirsService = F::FsCreateDirsService;
    type CommandExecutorService = F::CommandExecutorService;
    type InquireService = F::InquireService;
    type McpServer = F::McpServer;

    fn environment_service(&self) -> &Self::EnvironmentService {
        self.infra.environment_service()
    }

    fn file_read_service(&self) -> &Self::FsReadService {
        self.infra.file_read_service()
    }

    fn file_write_service(&self) -> &Self::FsWriteService {
        self.infra.file_write_service()
    }

    fn file_meta_service(&self) -> &Self::FsMetaService {
        self.infra.file_meta_service()
    }

    fn file_snapshot_service(&self) -> &Self::FsSnapshotService {
        self.infra.file_snapshot_service()
    }

    fn file_remove_service(&self) -> &Self::FsRemoveService {
        self.infra.file_remove_service()
    }

    fn create_dirs_service(&self) -> &Self::FsCreateDirsService {
        self.infra.create_dirs_service()
    }

    fn command_executor_service(&self) -> &Self::CommandExecutorService {
        self.infra.command_executor_service()
    }

    fn inquire_service(&self) -> &Self::InquireService {
        self.infra.inquire_service()
    }

    fn mcp_server(&self) -> &Self::McpServer {
        self.infra.mcp_server()
    }
}
