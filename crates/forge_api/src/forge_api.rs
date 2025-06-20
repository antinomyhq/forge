use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::{
    AuthService, ChatService, ConversationService, EnvironmentService, FileDiscoveryService,
    ForgeApp, KeyService, McpConfigManager, ProviderService, Services, WorkflowService,
};
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_services::{CommandInfra, ForgeServices};
use forge_stream::MpscStream;

use crate::API;

pub struct ForgeAPI<A, F> {
    app: Arc<A>,
    infra: Arc<F>,
}

impl<A, F> ForgeAPI<A, F> {
    pub fn new(app: Arc<A>, infra: Arc<F>) -> Self {
        Self { app, infra }
    }
}

impl ForgeAPI<ForgeServices<ForgeInfra>, ForgeInfra> {
    pub fn init(restricted: bool) -> Self {
        let infra = Arc::new(ForgeInfra::new(restricted));
        let app = Arc::new(ForgeServices::new(infra.clone()));
        ForgeAPI::new(app, infra)
    }
}

#[async_trait::async_trait]
impl<A: Services, F: CommandInfra> API for ForgeAPI<A, F> {
    async fn discover(&self) -> Result<Vec<File>> {
        self.app.collect(None).await
    }

    async fn tools(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let forge_app = ForgeApp::new(self.app.clone());
        forge_app.list_tools().await
    }

    async fn models(&self) -> Result<Vec<Model>> {
        Ok(self.app.models().await?)
    }

    async fn chat(
        &self,
        chat: ChatRequest,
    ) -> anyhow::Result<MpscStream<Result<ChatResponse, anyhow::Error>>> {
        // Create a ForgeApp instance and delegate the chat logic to it
        let forge_app = ForgeApp::new(self.app.clone());
        forge_app.chat(chat).await
    }

    async fn init_conversation<W: Into<Workflow> + Send + Sync>(
        &self,
        workflow: W,
    ) -> anyhow::Result<Conversation> {
        self.app.create_conversation(workflow.into()).await
    }

    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.app.upsert(conversation).await
    }

    async fn compact_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<CompactionResult> {
        let forge_app = ForgeApp::new(self.app.clone());
        forge_app.compact_conversation(conversation_id).await
    }

    fn environment(&self) -> Environment {
        self.app.get_environment().clone()
    }

    async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        self.app.read_workflow(path).await
    }

    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()> {
        self.app.write_workflow(path, workflow).await
    }

    async fn update_workflow<T>(&self, path: Option<&Path>, f: T) -> anyhow::Result<Workflow>
    where
        T: FnOnce(&mut Workflow) + Send,
    {
        self.app.update_workflow(path, f).await
    }

    async fn conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>> {
        self.app.find(conversation_id).await
    }

    async fn execute_shell_command(
        &self,
        command: &str,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        self.infra
            .execute_command(command.to_string(), working_dir)
            .await
    }
    async fn read_mcp_config(&self) -> Result<McpConfig> {
        self.app
            .read_mcp_config()
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn write_mcp_config(&self, scope: &Scope, config: &McpConfig) -> Result<()> {
        self.app
            .write_mcp_config(config, scope)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn execute_shell_command_raw(
        &self,
        command: &str,
    ) -> anyhow::Result<std::process::ExitStatus> {
        self.infra.execute_command_raw(command).await
    }

    async fn init_login(&self) -> Result<InitAuth> {
        self.app.auth_service().init().await
    }

    async fn login(&self, auth: &InitAuth) -> Result<()> {
        self.app.auth_service().login(auth).await
    }

    async fn logout(&self) -> Result<()> {
        self.app.auth_service().logout().await
    }

    async fn api_key(&self) -> Option<ForgeKey> {
        self.app.key_service().get().await
    }
    fn provider(&self, key: Option<ForgeKey>) -> Result<Provider> {
        self.app.get_provider(key).context("User isn't logged in")
    }
}
