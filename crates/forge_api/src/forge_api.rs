use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::dto::{InitAuth, LoginInfo, Provider, ProviderId, ToolsOverview};
use forge_app::{
    AgentLoaderService, AuthService, ConversationService, EnvironmentService, FileDiscoveryService,
    ForgeApp, McpConfigManager, McpService, ProviderRegistry, ProviderService, Services, User,
    UserUsage, Walker, WorkflowService,
};
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_services::{AppConfigRepository, CommandInfra, ForgeServices};
use forge_stream::MpscStream;

use crate::API;

pub struct ForgeAPI<S, F> {
    services: Arc<S>,
    infra: Arc<F>,
}

impl<A, F> ForgeAPI<A, F> {
    pub fn new(services: Arc<A>, infra: Arc<F>) -> Self {
        Self { services, infra }
    }
}

impl ForgeAPI<ForgeServices<ForgeInfra>, ForgeInfra> {
    pub fn init(restricted: bool, cwd: PathBuf) -> Self {
        let infra = Arc::new(ForgeInfra::new(restricted, cwd));
        let app = Arc::new(ForgeServices::new(infra.clone()));
        ForgeAPI::new(app, infra)
    }
}

#[async_trait::async_trait]
impl<A: Services, F: CommandInfra + AppConfigRepository> API for ForgeAPI<A, F> {
    async fn discover(&self) -> Result<Vec<File>> {
        let environment = self.services.get_environment();
        let config = Walker::unlimited().cwd(environment.cwd);
        self.services.collect_files(config).await
    }

    async fn tools(&self) -> anyhow::Result<ToolsOverview> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.list_tools().await
    }

    async fn models(&self) -> Result<Vec<Model>> {
        Ok(self
            .services
            .models(
                self.get_provider()
                    .await
                    .context("Failed to fetch models")?,
            )
            .await?)
    }
    async fn get_agents(&self) -> Result<Vec<Agent>> {
        Ok(self.services.get_agents().await?)
    }

    async fn providers(&self) -> Result<Vec<Provider>> {
        Ok(self.services.get_all_providers().await?)
    }

    async fn chat(
        &self,
        chat: ChatRequest,
    ) -> anyhow::Result<MpscStream<Result<ChatResponse, anyhow::Error>>> {
        // Create a ForgeApp instance and delegate the chat logic to it
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.chat(chat).await
    }

    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.services.upsert_conversation(conversation).await
    }

    async fn compact_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<CompactionResult> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.compact_conversation(conversation_id).await
    }

    fn environment(&self) -> Environment {
        self.services.get_environment().clone()
    }

    async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let app = ForgeApp::new(self.services.clone());
        app.read_workflow(path).await
    }

    async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let app = ForgeApp::new(self.services.clone());
        app.read_workflow_merged(path).await
    }

    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()> {
        let app = ForgeApp::new(self.services.clone());
        app.write_workflow(path, workflow).await
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

    async fn list_conversations(&self, limit: Option<usize>) -> anyhow::Result<Vec<Conversation>> {
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
    async fn read_mcp_config(&self) -> Result<McpConfig> {
        self.services
            .read_mcp_config()
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
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.init_auth().await
    }

    async fn login(&self, auth: &InitAuth) -> Result<()> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.login(auth).await
    }

    async fn logout(&self) -> Result<()> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.logout().await
    }
    async fn get_provider(&self) -> anyhow::Result<Provider> {
        self.services.get_active_provider().await
    }

    async fn set_provider(&self, provider_id: ProviderId) -> anyhow::Result<()> {
        self.services.set_active_provider(provider_id).await
    }

    async fn user_info(&self) -> Result<Option<User>> {
        let provider = self.get_provider().await?;
        if let Some(ref api_key) = provider.key {
            let user_info = self.services.user_info(api_key).await?;
            return Ok(Some(user_info));
        }
        Ok(None)
    }

    async fn user_usage(&self) -> Result<Option<UserUsage>> {
        let provider = self.get_provider().await?;
        if let Some(ref api_key) = provider.key {
            let user_usage = self.services.user_usage(api_key).await?;
            return Ok(Some(user_usage));
        }
        Ok(None)
    }

    async fn get_operating_agent(&self) -> Option<AgentId> {
        self.services.get_active_agent().await.ok().flatten()
    }

    async fn set_operating_agent(&self, agent_id: AgentId) -> anyhow::Result<()> {
        self.services.set_active_agent(agent_id).await
    }

    async fn get_operating_model(&self) -> Option<ModelId> {
        self.services.get_active_model().await.ok()
    }

    async fn set_operating_model(&self, model_id: ModelId) -> anyhow::Result<()> {
        self.services.set_active_model(model_id).await
    }

    async fn get_login_info(&self) -> Result<Option<LoginInfo>> {
        self.services.auth_service().get_auth_token().await
    }

    async fn reload_mcp(&self) -> Result<()> {
        self.services.mcp_service().reload_mcp().await
    }
}
