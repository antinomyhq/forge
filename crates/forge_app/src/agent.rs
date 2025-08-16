use std::path::{Path, PathBuf};
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

/// Get the git repository root directory if we're inside a git repository
async fn get_git_root(from_dir: &std::path::Path) -> Option<PathBuf> {
    let output = tokio::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(from_dir)
        .output()
        .await
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|root| PathBuf::from(root.trim()))
    } else {
        None
    }
}

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

/// Helper function to read AGENTS.md file if it exists and hasn't been
/// processed
async fn try_read_agents_file<T: FsReadService>(
    service: &T,
    path: &Path,
    label: &str,
    agents_content: &mut Vec<(String, String)>,
    processed_paths: &mut std::collections::HashSet<String>,
) {
    let path_str = path.to_string_lossy().to_string();
    if !processed_paths.contains(&path_str)
        && let Ok(output) = service.read(path_str.clone(), None, None).await {
            let crate::services::Content::File(content) = output.content;
            agents_content.push((label.to_string(), content));
            processed_paths.insert(path_str);
        }
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
        let mut processed_paths = std::collections::HashSet::new();
        let env = self.get_environment();

        // Machine root AGENTS.md first
        try_read_agents_file(
            self,
            &env.base_path.join("AGENTS.md"),
            "Machine root AGENTS.md",
            &mut agents_content,
            &mut processed_paths,
        )
        .await;

        // Project root AGENTS.md second
        if let Some(git_root_path) = get_git_root(&env.cwd).await {
            try_read_agents_file(
                self,
                &git_root_path.join("AGENTS.md"),
                "Project Root AGENTS.md",
                &mut agents_content,
                &mut processed_paths,
            )
            .await;
        }

        // Current directory AGENTS.md last
        try_read_agents_file(
            self,
            &env.cwd.join("AGENTS.md"),
            "Current Directory AGENTS.md",
            &mut agents_content,
            &mut processed_paths,
        )
        .await;

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
