use std::sync::Arc;

use forge_domain::App;

use crate::attachment::ForgeChatRequest;
use crate::conversation::ForgeConversationService;
use crate::provider::ForgeProviderService;
use crate::suggestion::ForgeSuggestionService;
use crate::template::ForgeTemplateService;
use crate::tool_service::ForgeToolService;
use crate::Infrastructure;

/// ForgeApp is the main application container that implements the App trait.
/// It provides access to all core services required by the application.
///
/// Type Parameters:
/// - F: The infrastructure implementation that provides core services like
///   environment, file reading, vector indexing, and embedding.
pub struct ForgeApp<F> {
    infra: Arc<F>,
    tool_service: ForgeToolService,
    provider_service: ForgeProviderService,
    conversation_service: ForgeConversationService,
    prompt_service: ForgeTemplateService,
    attachment_service: ForgeChatRequest,
    suggestion_service: Arc<ForgeSuggestionService<F>>,
}

impl<F: Infrastructure> ForgeApp<F> {
    pub fn new(infra: Arc<F>) -> Self {
        let suggestion_service = Arc::new(ForgeSuggestionService::new(infra.clone()));
        Self {
            infra: infra.clone(),
            tool_service: ForgeToolService::new(infra.clone(), suggestion_service.clone()),
            provider_service: ForgeProviderService::new(infra.clone()),
            conversation_service: ForgeConversationService::new(),
            prompt_service: ForgeTemplateService::new(),
            attachment_service: ForgeChatRequest,
            suggestion_service,
        }
    }
}

impl<F: Infrastructure> App for ForgeApp<F> {
    type ToolService = ForgeToolService;
    type ProviderService = ForgeProviderService;
    type ConversationService = ForgeConversationService;
    type PromptService = ForgeTemplateService;
    type AttachmentService = ForgeChatRequest;
    type SuggestionService = ForgeSuggestionService<F>;

    fn tool_service(&self) -> &Self::ToolService {
        &self.tool_service
    }

    fn suggestion_service(&self) -> &Self::SuggestionService {
        &self.suggestion_service
    }

    fn provider_service(&self) -> &Self::ProviderService {
        &self.provider_service
    }

    fn conversation_service(&self) -> &Self::ConversationService {
        &self.conversation_service
    }

    fn prompt_service(&self) -> &Self::PromptService {
        &self.prompt_service
    }

    fn attachment_service(&self) -> &Self::AttachmentService {
        &self.attachment_service
    }
}

impl<F: Infrastructure> Infrastructure for ForgeApp<F> {
    type EnvironmentService = F::EnvironmentService;
    type FileReadService = F::FileReadService;
    type VectorIndex = F::VectorIndex;
    type EmbeddingService = F::EmbeddingService;

    fn environment_service(&self) -> &Self::EnvironmentService {
        self.infra.environment_service()
    }

    fn file_read_service(&self) -> &Self::FileReadService {
        self.infra.file_read_service()
    }

    fn vector_index(&self) -> &Self::VectorIndex {
        self.infra.vector_index()
    }

    fn embedding_service(&self) -> &Self::EmbeddingService {
        self.infra.embedding_service()
    }
}
