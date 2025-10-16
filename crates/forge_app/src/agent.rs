use std::sync::Arc;

use forge_domain::{
    Agent, ChatCompletionMessage, Context, Conversation, ModelId, ResultStream, ToolCallContext,
    ToolCallFull, ToolResult,
};

use crate::file_tracking::FileChange;
use crate::tool_registry::ToolRegistry;
use crate::{ConversationService, ProviderRegistry, ProviderService, Services, TemplateService};

/// Agent service trait that provides core chat and tool call functionality.
/// This trait abstracts the essential operations needed by the Orchestrator.
#[async_trait::async_trait]
pub trait AgentService: Send + Sync + 'static {
    /// Execute a chat completion request
    async fn chat_agent(
        &self,
        id: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error>;

    /// Execute a tool call
    async fn call(
        &self,
        agent: &Agent,
        context: &ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult;

    /// Render a template with the provided object
    async fn render(
        &self,
        template: &str,
        object: &(impl serde::Serialize + Sync),
    ) -> anyhow::Result<String>;

    /// Synchronize the on-going conversation
    async fn update(&self, conversation: Conversation) -> anyhow::Result<()>;

    /// Detect files that have been modified externally by comparing current hashes
    /// with the hashes stored in conversation metrics
    async fn detect_file_changes(&self, conversation: &Conversation) -> Vec<crate::file_tracking::FileChange>;
}

/// Blanket implementation of AgentService for any type that implements Services
#[async_trait::async_trait]
impl<T: Services> AgentService for T {
    async fn chat_agent(
        &self,
        id: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let provider = self.get_active_provider().await?;
        self.chat(id, context, provider).await
    }

    async fn call(
        &self,
        agent: &Agent,
        context: &ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult {
        let registry = ToolRegistry::new(Arc::new(self.clone()));
        registry.call(agent, context, call).await
    }

    async fn render(
        &self,
        template: &str,
        object: &(impl serde::Serialize + Sync),
    ) -> anyhow::Result<String> {
        self.render_template(template, object).await
    }

    async fn update(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.upsert_conversation(conversation).await
    }

    async fn detect_file_changes(&self, conversation: &Conversation) -> Vec<FileChange> {
        use crate::file_tracking::FileChangeDetector;
        use std::collections::HashMap;

        let tracked_files: HashMap<String, String> = conversation
            .metrics
            .files_changed
            .iter()
            .map(|(path, metrics)| (path.clone(), metrics.file_hash.clone()))
            .collect();

        FileChangeDetector::new(Arc::new(self.clone()))
            .detect(&tracked_files)
            .await
    }
}
