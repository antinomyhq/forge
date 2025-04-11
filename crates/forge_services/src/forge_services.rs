use std::path::PathBuf;
use std::sync::Arc;

use forge_domain::Services;

use crate::attachment::ForgeChatRequest;
use crate::conversation::ForgeConversationService;
use crate::loader::ForgeLoaderService;
use crate::mcp::ForgeMcp;
use crate::provider::ForgeProviderService;
use crate::template::ForgeTemplateService;
use crate::tool_service::ForgeToolService;
use crate::Infrastructure;

/// ForgeApp is the main application container that implements the App trait.
/// It provides access to all core services required by the application.
///
/// Type Parameters:
/// - F: The infrastructure implementation that provides core services like
///   environment, file reading, vector indexing, and embedding.
#[derive(Clone)]
pub struct ForgeServices<F> {
    infra: Arc<F>,
    tool_service: Arc<ForgeToolService>,
    provider_service: ForgeProviderService,
    conversation_service: ForgeConversationService,
    prompt_service: ForgeTemplateService<F, ForgeToolService>,
    attachment_service: ForgeChatRequest<F>,
    loader: Arc<ForgeLoaderService<F>>,
    mcp_service: ForgeMcp,
}

impl<F: Infrastructure> ForgeServices<F> {
    pub fn new(infra: Arc<F>, workflow_path: Option<PathBuf>) -> Self {
        let tool_service = Arc::new(ForgeToolService::new(infra.clone()));
        let loader = Arc::new(ForgeLoaderService::new(infra.clone(), workflow_path));
        Self {
            infra: infra.clone(),
            provider_service: ForgeProviderService::new(infra.clone()),
            conversation_service: ForgeConversationService::new(),
            prompt_service: ForgeTemplateService::new(infra.clone(), tool_service.clone()),
            tool_service,
            attachment_service: ForgeChatRequest::new(infra.clone()),
            mcp_service: ForgeMcp::new(loader.clone()),
            loader,
        }
    }
}

impl<F: Infrastructure> Services for ForgeServices<F> {
    type ToolService = ForgeToolService;
    type ProviderService = ForgeProviderService;
    type ConversationService = ForgeConversationService;
    type TemplateService = ForgeTemplateService<F, ForgeToolService>;
    type AttachmentService = ForgeChatRequest<F>;
    type EnvironmentService = F::EnvironmentService;
    type LoaderService = ForgeLoaderService<F>;
    type McpService = ForgeMcp;

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
        &self.prompt_service
    }

    fn attachment_service(&self) -> &Self::AttachmentService {
        &self.attachment_service
    }

    fn environment_service(&self) -> &Self::EnvironmentService {
        self.infra.environment_service()
    }
    fn loader_service(&self) -> &ForgeLoaderService<F> {
        &self.loader
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
}
