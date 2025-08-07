use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::{
    AppConfig, AppConfigService, AuthService, ConversationService, EnvironmentService,
    FileDiscoveryService, ForgeApp, InitAuth, McpConfigManager, ProviderRegistry, ProviderService,
    Services, User, UserUsage, Walker, WorkflowService,
};
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_services::gcc::auto_manager::GccAutoManager;
use forge_services::gcc::storage::Storage as GccStorage;
use forge_services::{CommandInfra, ForgeServices};
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
impl<A: Services, F: CommandInfra> API for ForgeAPI<A, F> {
    async fn discover(&self) -> Result<Vec<File>> {
        let environment = self.services.get_environment();
        let config = Walker::unlimited().cwd(environment.cwd);
        self.services.collect_files(config).await
    }

    async fn tools(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.list_tools().await
    }

    async fn models(&self) -> Result<Vec<Model>> {
        Ok(self
            .services
            .models(self.provider().await.context("User is not logged in")?)
            .await?)
    }

    async fn chat(
        &self,
        chat: ChatRequest,
    ) -> anyhow::Result<MpscStream<Result<ChatResponse, anyhow::Error>>> {
        // Create a ForgeApp instance and delegate the chat logic to it
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.chat(chat).await
    }

    async fn init_conversation<W: Into<Workflow> + Send + Sync>(
        &self,
        workflow: W,
    ) -> anyhow::Result<Conversation> {
        self.services.create_conversation(workflow.into()).await
    }

    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.services.upsert(conversation).await
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
        self.services.read_workflow(path).await
    }

    async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        self.services.read_merged(path).await
    }

    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()> {
        self.services.write_workflow(path, workflow).await
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
        self.services.find(conversation_id).await
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
        self.infra.execute_command_raw(command, cwd).await
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
    async fn provider(&self) -> anyhow::Result<Provider> {
        self.services
            .get_provider(self.services.read_app_config().await.unwrap_or_default())
            .await
    }
    async fn app_config(&self) -> anyhow::Result<AppConfig> {
        self.services.read_app_config().await
    }

    async fn user_info(&self) -> Result<Option<User>> {
        let provider = self.provider().await?;
        if let Some(api_key) = provider.key() {
            let user_info = self.services.user_info(api_key).await?;
            return Ok(Some(user_info));
        }
        Ok(None)
    }

    async fn user_usage(&self) -> Result<Option<UserUsage>> {
        let provider = self.provider().await?;
        if let Some(api_key) = provider.key() {
            let user_usage = self.services.user_usage(api_key).await?;
            return Ok(Some(user_usage));
        }
        Ok(None)
    }

    // GCC (Git Context Controller) operations
    async fn gcc_init(&self) -> anyhow::Result<()> {
        let cwd = self.environment().cwd;
        GccStorage::init(&cwd).map_err(|e| anyhow::anyhow!("GCC init failed: {}", e))?;

        // Create main branch if it doesn't exist
        let main_branch_path = cwd.join(".GCC/branches/main");
        if !main_branch_path.exists() {
            GccStorage::create_branch(&cwd, "main")
                .map_err(|e| anyhow::anyhow!("Failed to create main branch: {}", e))?;
        }

        Ok(())
    }

    async fn gcc_commit(&self, message: &str) -> anyhow::Result<String> {
        let cwd = self.environment().cwd;

        // Initialize GCC if not already initialized
        self.gcc_init().await?;

        // Generate a unique commit ID using timestamp and hash
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let commit_id = format!(
            "{}-{}",
            timestamp,
            message
                .chars()
                .take(8)
                .collect::<String>()
                .replace(' ', "_")
        );

        // For now, use "main" as default branch - could be made configurable
        let branch = "main";

        // Create the commit content with message and timestamp
        let commit_content = format!(
            "# Commit: {commit_id}\n\nMessage: {message}\nTimestamp: {timestamp}\nBranch: {branch}\n"
        );

        // Write the commit
        GccStorage::write_commit(&cwd, branch, &commit_id, &commit_content)
            .map_err(|e| anyhow::anyhow!("GCC commit failed: {}", e))?;

        Ok(commit_id)
    }

    async fn gcc_create_branch(&self, name: &str) -> anyhow::Result<()> {
        let cwd = self.environment().cwd;

        // Initialize GCC if not already initialized
        self.gcc_init().await?;

        // Create the branch
        GccStorage::create_branch(&cwd, name)
            .map_err(|e| anyhow::anyhow!("GCC branch creation failed: {}", e))?;

        Ok(())
    }

    async fn gcc_read_context(&self, level: &str) -> anyhow::Result<String> {
        let cwd = self.environment().cwd;

        // Parse the level string into a ContextLevel
        let context_level = match level.trim() {
            "project" => forge_domain::ContextLevel::Project,
            level if level.contains('/') => {
                // Handle commit level like "branch/commit"
                forge_domain::ContextLevel::Commit(level.to_string())
            }
            branch_name => forge_domain::ContextLevel::Branch(branch_name.to_string()),
        };

        // Read the context
        let content = GccStorage::read_context(&cwd, &context_level)
            .map_err(|e| anyhow::anyhow!("GCC context read failed: {}", e))?;

        Ok(content)
    }

    /// Automatically manage GCC state based on conversation analysis
    async fn gcc_auto_manage(&self, conversation: &Conversation) -> anyhow::Result<String> {
        let cwd = self.environment().cwd;
        let auto_manager = GccAutoManager::new(&cwd);

        let actions = auto_manager
            .auto_manage(conversation)
            .await
            .map_err(|e| anyhow::anyhow!("GCC auto management failed: {}", e))?;

        // Format the result message
        let mut result_parts = Vec::new();

        if let Some(branch) = actions.branch_created {
            result_parts.push(format!("Created branch: {branch}"));
        }

        if let Some(branch) = actions.active_branch {
            result_parts.push(format!("Active branch: {branch}"));
        }

        if let Some(commit) = actions.commit_created {
            result_parts.push(format!("Created commit: {commit}"));
        }

        if actions.context_updated {
            result_parts.push("Updated context documentation".to_string());
        }

        let result = if result_parts.is_empty() {
            "GCC auto management completed (no actions taken)".to_string()
        } else {
            format!("GCC auto management completed: {}", result_parts.join(", "))
        };

        Ok(result)
    }

    /// Analyze a conversation for GCC insights without taking action
    async fn gcc_analyze_conversation(
        &self,
        conversation: &Conversation,
    ) -> anyhow::Result<String> {
        let cwd = self.environment().cwd;
        let auto_manager = GccAutoManager::new(&cwd);

        let analysis = auto_manager
            .analyze_conversation(conversation)
            .map_err(|e| anyhow::anyhow!("Conversation analysis failed: {}", e))?;

        // Format the analysis result
        let intent_desc = match &analysis.intent {
            forge_services::gcc::auto_manager::ConversationIntent::Feature { name } => {
                format!("Feature: {name}")
            }
            forge_services::gcc::auto_manager::ConversationIntent::BugFix { description } => {
                format!("Bug Fix: {description}")
            }
            forge_services::gcc::auto_manager::ConversationIntent::Refactoring { scope } => {
                format!("Refactoring: {scope}")
            }
            forge_services::gcc::auto_manager::ConversationIntent::Documentation { area } => {
                format!("Documentation: {area}")
            }
            forge_services::gcc::auto_manager::ConversationIntent::Exploration => {
                "Exploration".to_string()
            }
            forge_services::gcc::auto_manager::ConversationIntent::Mixed { primary: _ } => {
                "Mixed conversation".to_string()
            }
        };

        let result = format!(
            "Conversation Analysis:\n\
             Intent: {}\n\
             Complexity: {}/10\n\
             Suggested Branch: {}\n\
             Key Topics: {}\n\
             Summary: {}",
            intent_desc,
            analysis.complexity_score,
            analysis.suggested_branch_name,
            analysis.key_topics.join(", "),
            analysis.summary.chars().take(200).collect::<String>()
        );

        Ok(result)
    }
}
