use std::sync::Arc;

use forge_domain::{
    Agent, ChatCompletionMessage, Context, Conversation, ModelId, ResultStream, ToolCallContext,
    ToolCallFull, ToolResult,
};

use crate::tool_registry::ToolRegistry;
use crate::{
    AppConfigService, ConversationService, EnvironmentService, FsReadService, ProviderRegistry,
    ProviderService, Services, TemplateService,
};

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
        context: &mut ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult;

    /// Read AGENTS.md files and return them in order (repo root first, then
    /// current directory)
    async fn read_agents_md(&self) -> Vec<(String, String)>;

    /// Render a template with the provided object
    async fn render(
        &self,
        template: &str,
        object: &(impl serde::Serialize + Sync),
    ) -> anyhow::Result<String>;

    /// Synchronize the on-going conversation
    async fn update(&self, conversation: Conversation) -> anyhow::Result<()>;
}

/// Blanket implementation of AgentService for any type that implements Services
#[async_trait::async_trait]
impl<T: Services> AgentService for T {
    async fn chat_agent(
        &self,
        id: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let config = self.read_app_config().await.unwrap_or_default();
        let provider = self.get_provider(config).await?;
        self.chat(id, context, provider).await
    }

    async fn call(
        &self,
        agent: &Agent,
        context: &mut ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult {
        let registry = ToolRegistry::new(Arc::new(self.clone()));
        registry.call(agent, context, call).await
    }

    async fn read_agents_md(&self) -> Vec<(String, String)> {
        let mut agents_content = Vec::new();
        let env = self.get_environment();

        // 1. Check for AGENTS.md in repo root (base_path)
        let repo_agents_path = env.base_path.join("AGENTS.md");
        if let Ok(output) = self
            .read(repo_agents_path.to_string_lossy().to_string(), None, None)
            .await
        {
            let crate::services::Content::File(content) = output.content;
            agents_content.push(("Repository AGENTS.md".to_string(), content));
        }

        // 2. Check for AGENTS.md in current working directory (if different from
        //    base_path)
        if env.cwd != env.base_path {
            let cwd_agents_path = env.cwd.join("AGENTS.md");
            if let Ok(output) = self
                .read(cwd_agents_path.to_string_lossy().to_string(), None, None)
                .await
            {
                let crate::services::Content::File(content) = output.content;
                if !agents_content
                    .iter()
                    .any(|(_, existing_content)| existing_content == &content)
                {
                    agents_content.push(("Current Directory AGENTS.md".to_string(), content));
                }
            }
        }

        agents_content
    }

    async fn render(
        &self,
        template: &str,
        object: &(impl serde::Serialize + Sync),
    ) -> anyhow::Result<String> {
        self.render_template(template, object).await
    }

    async fn update(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.upsert(conversation).await
    }
}
