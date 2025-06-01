use std::sync::Arc;

use forge_domain::Services;

use crate::attachment::ForgeChatRequest;
use crate::compaction::ForgeCompactionService;
use crate::conversation::ForgeConversationService;
use crate::convo_manager_service::ForgeConversationSessionManager;
use crate::mcp::{ForgeMcpManager, ForgeMcpService};
use crate::provider::ForgeProviderService;
use crate::suggestion::ForgeSuggestionService;
use crate::template::ForgeTemplateService;
use crate::tool_service::ForgeToolService;
use crate::workflow::ForgeWorkflowService;
use crate::Infrastructure;

type McpService<F> = ForgeMcpService<ForgeMcpManager<F>, F>;
type ConversationService<F> = ForgeConversationService<
    ForgeCompactionService<ForgeTemplateService, ForgeProviderService>,
    McpService<F>,
    ForgeConversationSessionManager<F>,
>;

/// ForgeApp is the main application container that implements the App trait.
/// It provides access to all core services required by the application.
///
/// Type Parameters:
/// - F: The infrastructure implementation that provides core services like
///   environment, file reading, vector indexing, and embedding.
#[derive(Clone)]
pub struct ForgeServices<F> {
    infra: Arc<F>,
    tool_service: Arc<ForgeToolService<McpService<F>>>,
    provider_service: Arc<ForgeProviderService>,
    conversation_service: Arc<ConversationService<F>>,
    template_service: Arc<ForgeTemplateService>,
    attachment_service: Arc<ForgeChatRequest<F>>,
    compaction_service: Arc<ForgeCompactionService<ForgeTemplateService, ForgeProviderService>>,
    workflow_service: Arc<ForgeWorkflowService<F>>,
    suggestion_service: Arc<ForgeSuggestionService<F>>,
    mcp_manager: Arc<ForgeMcpManager<F>>,
    convo_session_manager: Arc<ForgeConversationSessionManager<F>>,
}

impl<F: Infrastructure> ForgeServices<F> {
    pub fn new(infra: Arc<F>) -> Self {
        let mcp_manager = Arc::new(ForgeMcpManager::new(infra.clone()));
        let mcp_service = Arc::new(ForgeMcpService::new(mcp_manager.clone(), infra.clone()));
        let tool_service = Arc::new(ForgeToolService::new(infra.clone(), mcp_service.clone()));
        let template_service = Arc::new(ForgeTemplateService::new());
        let provider_service = Arc::new(ForgeProviderService::new(infra.clone()));
        let attachment_service = Arc::new(ForgeChatRequest::new(infra.clone()));
        let compaction_service = Arc::new(ForgeCompactionService::new(
            template_service.clone(),
            provider_service.clone(),
        ));
        let convo_session_manager = Arc::new(ForgeConversationSessionManager::new(infra.clone()));

        let conversation_service = Arc::new(ForgeConversationService::new(
            compaction_service.clone(),
            mcp_service,
            convo_session_manager.clone(),
        ));

        let workflow_service = Arc::new(ForgeWorkflowService::new(infra.clone()));
        let suggestion_service = Arc::new(ForgeSuggestionService::new(infra.clone()));
        Self {
            infra,
            conversation_service,
            tool_service,
            attachment_service,
            compaction_service,
            provider_service,
            template_service,
            workflow_service,
            suggestion_service,
            mcp_manager,
            convo_session_manager,
        }
    }
}

impl<F: Infrastructure> Services for ForgeServices<F> {
    type ToolService = ForgeToolService<McpService<F>>;
    type ProviderService = ForgeProviderService;
    type ConversationService = ConversationService<F>;
    type TemplateService = ForgeTemplateService;
    type AttachmentService = ForgeChatRequest<F>;
    type EnvironmentService = F::EnvironmentService;
    type CompactionService = ForgeCompactionService<Self::TemplateService, Self::ProviderService>;
    type WorkflowService = ForgeWorkflowService<F>;
    type SuggestionService = ForgeSuggestionService<F>;
    type McpConfigManager = ForgeMcpManager<F>;
    type ConversationSessionManager = ForgeConversationSessionManager<F>;

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

    fn compaction_service(&self) -> &Self::CompactionService {
        self.compaction_service.as_ref()
    }

    fn workflow_service(&self) -> &Self::WorkflowService {
        self.workflow_service.as_ref()
    }

    fn suggestion_service(&self) -> &Self::SuggestionService {
        self.suggestion_service.as_ref()
    }

    fn mcp_config_manager(&self) -> &Self::McpConfigManager {
        self.mcp_manager.as_ref()
    }

    fn conversation_session_manager(&self) -> &Self::ConversationSessionManager {
        self.convo_session_manager.as_ref()
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
    type BufferService = F::BufferService;

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

    fn buffer_service(&self) -> &Self::BufferService {
        self.infra.buffer_service()
    }
}
