use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use forge_app::dto::ToolsOverview;
use forge_app::{
    AgentRegistry, AppConfigService, AuthService, CommandInfra, CommandLoaderService,
    ConversationService, EnvironmentInfra, EnvironmentService, FileDiscoveryService, ForgeApp,
    McpConfigManager, McpService, ProviderAuthService, ProviderService, Services, User, UserUsage,
    Walker, WorkflowService,
};
use forge_domain::{InitAuth, LoginInfo, *};
use forge_infra::ForgeInfra;
use forge_repo::ForgeRepo;
use forge_services::ForgeServices;
use forge_stream::MpscStream;
use url::Url;

use crate::API;

pub struct ForgeAPI<S, F> {
    services: Arc<S>,
    infra: Arc<F>,
}

impl<A, F> ForgeAPI<A, F> {
    pub fn new(services: Arc<A>, infra: Arc<F>) -> Self {
        Self { services, infra }
    }

    /// Creates a ForgeApp instance with the current services
    fn app(&self) -> ForgeApp<A>
    where
        A: Services,
    {
        ForgeApp::new(self.services.clone())
    }
}

impl ForgeAPI<ForgeServices<ForgeRepo<ForgeInfra>>, ForgeInfra> {
    pub fn init(restricted: bool, cwd: PathBuf) -> Self {
        let infra = Arc::new(ForgeInfra::new(restricted, cwd));
        let repo = Arc::new(ForgeRepo::new(infra.clone()));
        let app = Arc::new(ForgeServices::new(repo.clone()));
        ForgeAPI::new(app, infra)
    }
}

#[async_trait::async_trait]
impl<A: Services, F: CommandInfra + EnvironmentInfra> API for ForgeAPI<A, F> {
    async fn discover(&self) -> Result<Vec<File>> {
        let environment = self.services.get_environment();
        let config = Walker::unlimited().cwd(environment.cwd);
        self.services.collect_files(config).await
    }

    async fn get_tools(&self) -> anyhow::Result<ToolsOverview> {
        self.app().list_tools().await
    }

    async fn get_models(&self) -> Result<Vec<Model>> {
        Ok(self
            .services
            .models(
                self.get_default_provider()
                    .await
                    .context("Failed to fetch models")?,
            )
            .await?)
    }
    async fn get_agents(&self) -> Result<Vec<Agent>> {
        Ok(self.services.get_agents().await?)
    }

    async fn get_providers(&self) -> Result<Vec<AnyProvider>> {
        Ok(self.services.get_all_providers().await?)
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
        self.app().compact_conversation(conversation_id).await
    }

    fn environment(&self) -> Environment {
        self.services.get_environment().clone()
    }

    async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        self.app().read_workflow(path).await
    }

    async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        self.app().read_workflow_merged(path).await
    }

    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()> {
        self.app().write_workflow(path, workflow).await
    }

    async fn update_workflow<T>(&self, path: Option<&Path>, f: T) -> anyhow::Result<Workflow>
    where
        T: FnOnce(&mut Workflow) + Send,
    {
        self.services.update_workflow(path, f).await
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

    async fn execute_shell_command(
        &self,
        command: &str,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        self.infra
            .execute_command(command.to_string(), working_dir, false, None)
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
        self.infra.execute_command_raw(command, cwd, None).await
    }

    async fn init_login(&self) -> Result<InitAuth> {
        self.app().init_auth().await
    }

    async fn login(&self, auth: &InitAuth) -> Result<()> {
        self.app().login(auth).await
    }

    async fn logout(&self) -> Result<()> {
        self.app().logout().await
    }
    async fn get_agent_provider(&self, agent_id: AgentId) -> anyhow::Result<Provider<Url>> {
        self.app().get_provider(Some(agent_id)).await
    }

    async fn get_default_provider(&self) -> anyhow::Result<Provider<Url>> {
        self.app().get_provider(None).await
    }

    async fn set_default_provider(&self, provider_id: ProviderId) -> anyhow::Result<()> {
        self.services.set_default_provider(provider_id).await
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
        self.app().get_model(Some(agent_id)).await.ok()
    }

    async fn get_default_model(&self) -> Option<ModelId> {
        self.app().get_model(None).await.ok()
    }
    async fn set_default_model(
        &self,
        agent_id: Option<AgentId>,
        model_id: ModelId,
    ) -> anyhow::Result<()> {
        self.app().set_default_model(agent_id, model_id).await
    }

    async fn get_login_info(&self) -> Result<Option<LoginInfo>> {
        self.services.auth_service().get_auth_token().await
    }

    async fn reload_mcp(&self) -> Result<()> {
        self.services.mcp_service().reload_mcp().await
    }
    async fn get_commands(&self) -> Result<Vec<Command>> {
        self.services.get_commands().await
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
        Ok(self.services.remove_credential(provider_id).await?)
    }
}
