use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_services::{CommandExecutorService, ForgeServices, Infrastructure};
use forge_stream::MpscStream;
use tracing::error;

pub struct ForgeAPI<F> {
    app: Arc<F>,
}

impl<F: Services + Infrastructure> ForgeAPI<F> {
    pub fn new(app: Arc<F>) -> Self {
        Self { app: app.clone() }
    }
}

impl ForgeAPI<ForgeServices<ForgeInfra>> {
    pub fn init(restricted: bool) -> Self {
        let infra = Arc::new(ForgeInfra::new(restricted));
        let app = Arc::new(ForgeServices::new(infra));
        ForgeAPI::new(app)
    }
}

#[async_trait::async_trait]
impl<F: Services + Infrastructure> API for ForgeAPI<F> {
    async fn suggestions(&self) -> Result<Vec<File>> {
        self.app.suggestion_service().suggestions().await
    }

    async fn tools(&self) -> Vec<ToolDefinition> {
        self.app.tool_service().list()
    }

    async fn models(&self) -> Result<Vec<Model>> {
        Ok(self.app.provider_service().models().await?)
    }

    async fn chat(
        &self,
        chat: ChatRequest,
    ) -> anyhow::Result<MpscStream<Result<AgentMessage<ChatResponse>, anyhow::Error>>> {
        let app = self.app.clone();
        let conversation = app
            .conversation_service()
            .find(&chat.conversation_id)
            .await
            .unwrap_or_default()
            .expect("conversation for the request should've been created at this point.");

        Ok(MpscStream::spawn(move |tx| async move {
            let tx = Arc::new(tx);

            let orch = Orchestrator::new(app, conversation, Some(tx.clone()));

            if let Err(err) = orch.dispatch(chat.event).await {
                if let Err(e) = tx.send(Err(err)).await {
                    error!("Failed to send error to stream: {:#?}", e);
                }
            }
        }))
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
        self.app
            .command_executor_service()
            .execute_command(command.to_string(), working_dir)
            .await
    }
}
