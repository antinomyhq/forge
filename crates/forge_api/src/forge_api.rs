use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use forge_app::{
    ConsoleService, ConversationService, ConversationSessionManager, EnvironmentService,
    FileDiscoveryService, ForgeApp, McpConfigManager, ProviderService, Services, WorkflowService,
};
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_services::{CommandExecutorService, ForgeServices, Infrastructure};
use forge_stream::MpscStream;

use crate::API;

pub struct ForgeAPI<A, F> {
    app: Arc<A>,
    infra: Arc<F>,
}

impl<A: Services, F: Infrastructure> ForgeAPI<A, F> {
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
impl<A: Services, F: Infrastructure> API for ForgeAPI<A, F> {
    async fn discover(&self) -> Result<Vec<File>> {
        self.app.file_discovery_service().collect(None).await
    }

    async fn tools(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let forge_app = ForgeApp::new(self.app.clone());
        forge_app.list_tools().await
    }

    async fn models(&self) -> Result<Vec<Model>> {
        Ok(self.app.provider_service().models().await?)
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
        self.app
            .conversation_service()
            .create(workflow.into())
            .await
    }

    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.app.conversation_service().upsert(conversation).await
    }

    async fn compact_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<CompactionResult> {
        let forge_app = ForgeApp::new(self.app.clone());
        forge_app.compact_conversation(conversation_id).await
    }

    fn environment(&self) -> Environment {
        Services::environment_service(self.app.as_ref())
            .get_environment()
            .clone()
    }

    async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        self.app.workflow_service().read(path).await
    }

    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()> {
        self.app.workflow_service().write(path, workflow).await
    }

    async fn update_workflow<T>(&self, path: Option<&Path>, f: T) -> anyhow::Result<Workflow>
    where
        T: FnOnce(&mut Workflow) + Send,
    {
        self.app.workflow_service().update_workflow(path, f).await
    }

    async fn conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>> {
        self.app.conversation_service().find(conversation_id).await
    }

    async fn execute_shell_command(
        &self,
        command: &str,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        self.infra
            .command_executor_service()
            .execute_command(command.to_string(), working_dir)
            .await
    }
    async fn read_mcp_config(&self) -> Result<McpConfig> {
        self.app
            .mcp_config_manager()
            .read()
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn write_mcp_config(&self, scope: &Scope, config: &McpConfig) -> Result<()> {
        self.app
            .mcp_config_manager()
            .write(config, scope)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn execute_shell_command_raw(
        &self,
        command: &str,
    ) -> anyhow::Result<std::process::ExitStatus> {
        self.infra
            .command_executor_service()
            .execute_command_raw(command)
            .await
    }

    async fn restore_conversation(&self) -> Result<Conversation> {
        self.app.conversation_session_manager().load().await
    }

    async fn restore_buffer_state(&self, buffer_size: usize) -> Result<Vec<Buffer>> {
        self.app
            .conversation_session_manager()
            .state(buffer_size)
            .await
    }

    async fn set_buffer_state(&self, state: Buffer) -> Result<()> {
        self.app
            .conversation_session_manager()
            .buffer_update(state)
            .await
    }

    async fn clear_state(&self) -> Result<()> {
        self.app.conversation_session_manager().clear().await
    }

    async fn print(&self, text: &str) -> Result<()> {
        self.app.console_service().print(text).await
    }
}
