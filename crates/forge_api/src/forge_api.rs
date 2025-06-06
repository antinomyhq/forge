use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_services::{
    AttachmentService, CommandExecutorService, ConsoleService, ConversationService,
    ConversationSessionManager, EnvironmentService, ForgeServices, Infrastructure,
    McpConfigManager, ProviderService, Services, SuggestionService, ToolService, WorkflowService,
};
use forge_stream::MpscStream;
use tracing::error;

pub struct ForgeAPI<A, F> {
    app: Arc<A>,
    infra: Arc<F>,
}

impl<A: Services + AgentService, F: Infrastructure> ForgeAPI<A, F> {
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
impl<A: Services + AgentService, F: Infrastructure> API for ForgeAPI<A, F> {
    async fn suggestions(&self) -> Result<Vec<File>> {
        self.app.suggestion_service().suggestions().await
    }

    async fn tools(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.app.tool_service().list().await
    }

    async fn models(&self) -> Result<Vec<Model>> {
        Ok(self.app.provider_service().models().await?)
    }

    async fn chat(
        &self,
        mut chat: ChatRequest,
    ) -> anyhow::Result<MpscStream<Result<ChatResponse, anyhow::Error>>> {
        let app = self.app.clone();
        let conversation = app
            .conversation_service()
            .find(&chat.conversation_id)
            .await
            .unwrap_or_default()
            .expect("conversation for the request should've been created at this point.");

        let tool_definitions = app.tool_service().list().await?;
        let models = app.provider_service().models().await?;

        // Always try to get attachments and overwrite them
        let attachments = app
            .attachment_service()
            .attachments(&chat.event.value.to_string())
            .await?;
        chat.event = chat.event.attachments(attachments);
        let orch = Orchestrator::new(
            app.clone(),
            app.environment_service().get_environment().clone(),
            conversation,
        )
        .tool_definitions(tool_definitions)
        .models(models);

        let stream = MpscStream::spawn(|tx| {
            async move {
                let tx = Arc::new(tx);

                // Execute dispatch and always save conversation afterwards
                let mut orch = orch.sender(tx.clone());
                let dispatch_result = orch.dispatch(chat.event).await;

                // Always save conversation using get_conversation()
                let conversation = orch.get_conversation().clone();
                let save_result = app.conversation_service().upsert(conversation).await;

                // Send any error to the stream (prioritize dispatch error over save error)
                if let Some(err) = dispatch_result.err().or(save_result.err()) {
                    if let Err(e) = tx.send(Err(err)).await {
                        error!("Failed to send error to stream: {:#?}", e);
                    }
                }
            }
        });

        Ok(stream)
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
        self.app
            .conversation_service()
            .compact_conversation(conversation_id)
            .await
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

    async fn restore_buffer_state(&self, n: usize) -> Result<Vec<Buffer>> {
        self.app.conversation_session_manager().state(n).await
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
