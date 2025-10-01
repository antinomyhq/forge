use std::sync::Arc;

use async_trait::async_trait;
use forge_app::config_resolver::{
    ConfigError, ConfigSource, ConfigurationResolver, ResolvedConfig,
};
use forge_app::domain::{AgentId, ModelId};
use forge_app::{AgentLoaderService, AppConfigService, Services};

use crate::agent_loader::AgentLoaderService as ForgeAgentLoaderService;
use crate::app_config::ForgeConfigService;
use crate::attachment::ForgeChatRequest;
use crate::auth::ForgeAuthService;
use crate::conversation::ForgeConversationService;
use crate::custom_instructions::ForgeCustomInstructionsService;
use crate::discovery::ForgeDiscoveryService;
use crate::env::ForgeEnvironmentService;
use crate::infra::HttpInfra;
use crate::mcp::{ForgeMcpManager, ForgeMcpService};
use crate::policy::ForgePolicyService;
use crate::provider::{ForgeProviderRegistry, ForgeProviderService};
use crate::template::ForgeTemplateService;
use crate::tool_services::{
    ForgeFetch, ForgeFollowup, ForgeFsCreate, ForgeFsPatch, ForgeFsRead, ForgeFsRemove,
    ForgeFsSearch, ForgeFsUndo, ForgePlanCreate, ForgeShell,
};
use crate::workflow::ForgeWorkflowService;
use crate::{
    CommandInfra, ConversationRepository, DirectoryReaderInfra, EnvironmentInfra,
    FileDirectoryInfra, FileInfoInfra, FileReaderInfra, FileRemoverInfra, FileWriterInfra,
    McpServerInfra, SnapshotInfra, UserInfra, WalkerInfra,
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
pub struct ForgeServices<
    F: HttpInfra
        + EnvironmentInfra
        + McpServerInfra
        + WalkerInfra
        + SnapshotInfra
        + FileRemoverInfra
        + FileDirectoryInfra,
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
    config_service: Arc<ForgeConfigService<F>>,
    auth_service: Arc<AuthService<F>>,
    provider_service: Arc<ForgeProviderRegistry<F>>,
    agent_loader_service: Arc<ForgeAgentLoaderService<F>>,
    policy_service: ForgePolicyService<F>,
    environment: Vec<String>,
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
        + ConversationRepository
        + SnapshotInfra
        + FileRemoverInfra
        + FileDirectoryInfra
        + Clone,
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
        let config_service = Arc::new(ForgeConfigService::new(infra.clone()));
        let auth_service = Arc::new(ForgeAuthService::new(infra.clone()));
        let chat_service = Arc::new(ForgeProviderService::<F>::new(infra.clone()));
        let file_create_service = Arc::new(ForgeFsCreate::new(infra.clone()));
        let plan_create_service = Arc::new(ForgePlanCreate::new(infra.clone()));
        let file_read_service = Arc::new(ForgeFsRead::new(infra.clone()));
        let file_search_service = Arc::new(ForgeFsSearch::new(infra.clone()));
        let file_remove_service = Arc::new(ForgeFsRemove::new(infra.clone()));
        let file_patch_service = Arc::new(ForgeFsPatch::new(infra.clone()));
        let file_undo_service = Arc::new(ForgeFsUndo::new(infra.clone()));
        let shell_service = Arc::new(ForgeShell::new(infra.clone()));
        let fetch_service = Arc::new(ForgeFetch::new());
        let followup_service = Arc::new(ForgeFollowup::new(infra.clone()));
        let provider_service = Arc::new(ForgeProviderRegistry::new(infra.clone()));
        let env_service = Arc::new(ForgeEnvironmentService::new(infra.clone()));
        let custom_instructions_service =
            Arc::new(ForgeCustomInstructionsService::new(infra.clone()));
        let agent_loader_service = Arc::new(ForgeAgentLoaderService::new(infra.clone()));
        let policy_service = ForgePolicyService::new(infra.clone());
        let environment = std::env::vars()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

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
            config_service,
            auth_service,
            chat_service,
            provider_service,
            agent_loader_service,
            policy_service,
            environment,
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
        + DirectoryReaderInfra
        + HttpInfra
        + WalkerInfra
        + ConversationRepository
        + Clone,
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
    type AgentLoaderService = ForgeAgentLoaderService<F>;
    type PolicyService = ForgePolicyService<F>;
    type ConfigurationResolver = Self;

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

    fn app_config_service(&self) -> &Self::AppConfigService {
        self.config_service.as_ref()
    }

    fn provider_registry(&self) -> &Self::ProviderRegistry {
        &self.provider_service
    }
    fn agent_loader_service(&self) -> &Self::AgentLoaderService {
        &self.agent_loader_service
    }

    fn policy_service(&self) -> &Self::PolicyService {
        &self.policy_service
    }

    fn configuration_resolver(&self) -> &Self::ConfigurationResolver {
        self
    }
}

#[async_trait]
impl<F> ConfigurationResolver for ForgeServices<F>
where
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
        + ConversationRepository
        + SnapshotInfra
        + FileRemoverInfra
        + FileDirectoryInfra
        + Clone
        + 'static,
{
    async fn resolve_agent(&self) -> Result<Option<(AgentId, ConfigSource)>, ConfigError> {
        // Environment variables have highest precedence
        if let Some(agent) = self.get_agent_from_env() {
            return Ok(Some((agent, ConfigSource::Environment)));
        }

        // Then app config
        if let Some(config) = self.config_service.get_app_config().await
            && let Some(agent) = config.operating_agent
            && !agent.as_str().is_empty()
        {
            return Ok(Some((agent, ConfigSource::AppConfig)));
        }

        // Finally default
        Ok(Some((AgentId::default(), ConfigSource::Default)))
    }

    async fn resolve_model(&self) -> Result<Option<(ModelId, ConfigSource)>, ConfigError> {
        // Environment variables have highest precedence
        if let Some(model) = self.get_model_from_env() {
            return Ok(Some((model, ConfigSource::Environment)));
        }

        // Then app config
        if let Some(config) = self.config_service.get_app_config().await
            && let Some(model) = config.operating_model
            && !model.as_str().is_empty()
        {
            return Ok(Some((model, ConfigSource::AppConfig)));
        }

        // No default model - user must select one
        Ok(None)
    }

    async fn resolve_config(&self) -> Result<ResolvedConfig, ConfigError> {
        let agent = self.resolve_agent().await?;
        let model = self.resolve_model().await?;

        Ok(ResolvedConfig { agent, model })
    }

    async fn validate_resolved_config(&self, config: &ResolvedConfig) -> Result<(), ConfigError> {
        // Validate that the agent exists if specified
        if let Some((agent, _)) = &config.agent {
            let agents = self.agent_loader_service.get_agents().await?;
            if !agents.iter().any(|a| a.id == *agent) {
                return Err(ConfigError::AgentNotAvailable(agent.clone()));
            }
        }

        // TODO: Validate model availability when we have a proper way to access
        // provider For now, we'll skip model validation to avoid provider
        // service complexity

        // Validate model-agent compatibility if both are specified
        if let (Some((agent, _)), Some((model, _))) = (&config.agent, &config.model)
            && let Some(incompatibility) =
                self.check_agent_model_compatibility(agent, model).await?
        {
            return Err(incompatibility);
        }

        Ok(())
    }

    async fn get_app_config(&self) -> Result<Option<forge_app::dto::AppConfig>, ConfigError> {
        Ok(self.config_service.get_app_config().await)
    }

    async fn set_app_config(&self, config: &forge_app::dto::AppConfig) -> Result<(), ConfigError> {
        // Validate the configuration before setting
        if let Err(e) = config.validate() {
            return Err(ConfigError::ValidationFailed(e.to_string()));
        }

        self.config_service.set_app_config(config).await?;
        Ok(())
    }
}

impl<F> ForgeServices<F>
where
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
        + ConversationRepository
        + SnapshotInfra
        + FileRemoverInfra
        + FileDirectoryInfra
        + Clone,
{
    fn get_agent_from_env(&self) -> Option<AgentId> {
        self.environment.iter().find_map(|line| {
            line.strip_prefix("FORGE_AGENT=")
                .map(|agent| AgentId::new(agent.to_string()))
        })
    }

    fn get_model_from_env(&self) -> Option<ModelId> {
        self.environment.iter().find_map(|line| {
            line.strip_prefix("FORGE_MODEL=")
                .map(|model| ModelId::new(model.to_string()))
        })
    }

    async fn check_agent_model_compatibility(
        &self,
        agent: &AgentId,
        _model: &ModelId,
    ) -> Result<Option<ConfigError>, ConfigError> {
        // Get the agent to check its capabilities
        let agents = self.agent_loader_service.get_agents().await?;
        let _agent_info = agents.iter().find(|a| a.id == *agent);

        // TODO: Implement agent-model compatibility checking
        // For now, we'll assume all agents are compatible with all models
        // In the future, we might check agent.allowed_models if it exists

        Ok(None)
    }
}
