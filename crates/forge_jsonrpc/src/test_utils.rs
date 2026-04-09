use std::path::PathBuf;
use std::sync::Arc;

use forge_api::UserUsage;
use forge_app::User;
use forge_domain::{
    AgentId, ChatRequest, CommandOutput, Conversation, ConversationId, McpConfig, ProviderId,
    Scope, WorkspaceId, WorkspaceInfo,
};
use futures::stream::BoxStream;
use serde_json::{Value, json};

/// Mock API implementation for testing
#[derive(Default)]
pub struct MockAPI {
    pub models: Vec<forge_domain::Model>,
    pub agents: Vec<forge_domain::Agent>,
    pub conversations: Vec<Conversation>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub authenticated: bool,
}


#[async_trait::async_trait]
impl forge_api::API for MockAPI {
    async fn discover(&self) -> anyhow::Result<Vec<forge_domain::File>> {
        Ok(vec![
            forge_domain::File { path: "test.txt".to_string(), is_dir: false },
            forge_domain::File { path: "src/main.rs".to_string(), is_dir: false },
        ])
    }

    async fn get_tools(&self) -> anyhow::Result<forge_api::ToolsOverview> {
        Ok(forge_api::ToolsOverview {
            system: vec![],
            agents: vec![],
            mcp: forge_domain::McpServers::default(),
        })
    }

    async fn get_models(&self) -> anyhow::Result<Vec<forge_domain::Model>> {
        Ok(self.models.clone())
    }

    async fn get_all_provider_models(&self) -> anyhow::Result<Vec<forge_domain::ProviderModels>> {
        Ok(vec![])
    }

    async fn get_agents(&self) -> anyhow::Result<Vec<forge_domain::Agent>> {
        Ok(self.agents.clone())
    }

    async fn get_providers(&self) -> anyhow::Result<Vec<forge_domain::AnyProvider>> {
        Ok(vec![])
    }

    async fn get_provider(&self, _id: &ProviderId) -> anyhow::Result<forge_domain::AnyProvider> {
        anyhow::bail!("Provider not found")
    }

    async fn chat(
        &self,
        _chat: ChatRequest,
    ) -> anyhow::Result<forge_stream::MpscStream<anyhow::Result<forge_domain::ChatResponse>>> {
        Ok(forge_stream::MpscStream::spawn(|sender| async move {
            let _ = sender
                .send(Ok(forge_domain::ChatResponse::TaskComplete))
                .await;
        }))
    }

    async fn commit(
        &self,
        _preview: bool,
        _max_diff_size: Option<usize>,
        _diff: Option<String>,
        _additional_context: Option<String>,
    ) -> anyhow::Result<forge_app::CommitResult> {
        Ok(forge_app::CommitResult {
            message: "test commit".to_string(),
            committed: false,
            has_staged_files: true,
            git_output: String::new(),
        })
    }

    fn environment(&self) -> forge_domain::Environment {
        forge_domain::Environment {
            os: std::env::consts::OS.to_string(),
            cwd: std::path::PathBuf::from("."),
            home: dirs::home_dir(),
            shell: if cfg!(target_os = "windows") {
                std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
            } else {
                std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
            },
            base_path: dirs::home_dir()
                .map(|h| h.join(".forge"))
                .unwrap_or_else(|| std::path::PathBuf::from(".forge")),
        }
    }

    async fn upsert_conversation(&self, _conversation: Conversation) -> anyhow::Result<()> {
        Ok(())
    }

    async fn conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>> {
        Ok(self
            .conversations
            .iter()
            .find(|c| c.id == *conversation_id)
            .cloned())
    }

    async fn get_conversations(&self, _limit: Option<usize>) -> anyhow::Result<Vec<Conversation>> {
        Ok(self.conversations.clone())
    }

    async fn last_conversation(&self) -> anyhow::Result<Option<Conversation>> {
        Ok(self.conversations.last().cloned())
    }

    async fn delete_conversation(&self, _conversation_id: &ConversationId) -> anyhow::Result<()> {
        Ok(())
    }

    async fn rename_conversation(
        &self,
        _conversation_id: &ConversationId,
        _title: String,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn compact_conversation(
        &self,
        _conversation_id: &ConversationId,
    ) -> anyhow::Result<forge_domain::CompactionResult> {
        Ok(forge_domain::CompactionResult {
            original_tokens: 1000,
            compacted_tokens: 500,
            original_messages: 20,
            compacted_messages: 10,
        })
    }

    async fn execute_shell_command(
        &self,
        command: &str,
        _working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            command: command.to_string(),
            stdout: format!("Executed: {}", command),
            stderr: String::new(),
            exit_code: Some(0),
        })
    }

    async fn execute_shell_command_raw(
        &self,
        _command: &str,
    ) -> anyhow::Result<std::process::ExitStatus> {
        Ok(std::process::ExitStatus::default())
    }

    async fn read_mcp_config(&self, _scope: Option<&Scope>) -> anyhow::Result<McpConfig> {
        Ok(McpConfig::default())
    }

    async fn write_mcp_config(&self, _scope: &Scope, _config: &McpConfig) -> anyhow::Result<()> {
        Ok(())
    }

    async fn get_agent_provider(
        &self,
        _agent_id: AgentId,
    ) -> anyhow::Result<forge_domain::Provider<url::Url>> {
        anyhow::bail!("Not implemented")
    }

    async fn get_default_provider(&self) -> anyhow::Result<forge_domain::Provider<url::Url>> {
        anyhow::bail!("Not implemented")
    }

    async fn update_config(&self, _ops: Vec<forge_domain::ConfigOperation>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn user_info(&self) -> anyhow::Result<Option<User>> {
        if self.authenticated {
            Ok(Some(User {
                auth_provider_id: forge_app::AuthProviderId::new("test"),
            }))
        } else {
            Ok(None)
        }
    }

    async fn user_usage(&self) -> anyhow::Result<Option<UserUsage>> {
        Ok(None)
    }

    async fn get_active_agent(&self) -> Option<AgentId> {
        None
    }

    async fn set_active_agent(&self, _agent_id: AgentId) -> anyhow::Result<()> {
        Ok(())
    }

    async fn get_agent_model(&self, _agent_id: AgentId) -> Option<forge_domain::ModelId> {
        None
    }

    async fn get_default_model(&self) -> Option<forge_domain::ModelId> {
        None
    }

    async fn get_commit_config(&self) -> anyhow::Result<Option<forge_domain::ModelConfig>> {
        Ok(None)
    }

    async fn get_suggest_config(&self) -> anyhow::Result<Option<forge_domain::ModelConfig>> {
        Ok(None)
    }

    async fn get_reasoning_effort(&self) -> anyhow::Result<Option<forge_domain::Effort>> {
        Ok(None)
    }

    async fn reload_mcp(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn get_commands(&self) -> anyhow::Result<Vec<forge_domain::Command>> {
        Ok(vec![])
    }

    async fn get_skills(&self) -> anyhow::Result<Vec<forge_api::Skill>> {
        Ok(vec![])
    }

    async fn generate_command(&self, _prompt: forge_domain::UserPrompt) -> anyhow::Result<String> {
        Ok("echo hello".to_string())
    }

    async fn init_provider_auth(
        &self,
        _provider_id: ProviderId,
        _method: forge_domain::AuthMethod,
    ) -> anyhow::Result<forge_domain::AuthContextRequest> {
        Ok(forge_domain::AuthContextRequest::ApiKey(
            forge_domain::ApiKeyRequest {
                required_params: vec![],
                existing_params: None,
                api_key: None,
            },
        ))
    }

    async fn complete_provider_auth(
        &self,
        _provider_id: ProviderId,
        _context: forge_domain::AuthContextResponse,
        _timeout: std::time::Duration,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn remove_provider(&self, _provider_id: &ProviderId) -> anyhow::Result<()> {
        Ok(())
    }

    async fn sync_workspace(
        &self,
        _path: PathBuf,
    ) -> anyhow::Result<forge_stream::MpscStream<anyhow::Result<forge_domain::SyncProgress>>> {
        Ok(forge_stream::MpscStream::spawn(|sender| async move {
            let _ = sender
                .send(Ok(forge_domain::SyncProgress::Syncing {
                    current: 1,
                    total: 10,
                }))
                .await;
        }))
    }

    async fn query_workspace(
        &self,
        _path: PathBuf,
        _params: forge_domain::SearchParams<'_>,
    ) -> anyhow::Result<Vec<forge_domain::Node>> {
        Ok(vec![])
    }

    async fn list_workspaces(&self) -> anyhow::Result<Vec<WorkspaceInfo>> {
        Ok(self.workspaces.clone())
    }

    async fn get_workspace_info(&self, _path: PathBuf) -> anyhow::Result<Option<WorkspaceInfo>> {
        Ok(self.workspaces.first().cloned())
    }

    async fn delete_workspaces(&self, _workspace_ids: Vec<WorkspaceId>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn get_workspace_status(
        &self,
        _path: PathBuf,
    ) -> anyhow::Result<Vec<forge_domain::FileStatus>> {
        Ok(vec![])
    }

    fn hydrate_channel(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn is_authenticated(&self) -> anyhow::Result<bool> {
        Ok(self.authenticated)
    }

    async fn create_auth_credentials(&self) -> anyhow::Result<forge_domain::WorkspaceAuth> {
        Ok(forge_domain::WorkspaceAuth::new(
            forge_domain::UserId::generate(),
            "test".to_string().into(),
        ))
    }

    async fn init_workspace(&self, _path: PathBuf) -> anyhow::Result<WorkspaceId> {
        Ok(WorkspaceId::from_string("test-workspace").unwrap())
    }

    async fn migrate_env_credentials(
        &self,
    ) -> anyhow::Result<Option<forge_domain::MigrationResult>> {
        Ok(None)
    }

    async fn generate_data(
        &self,
        _data_parameters: forge_domain::DataGenerationParameters,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<Value>>> {
        use futures::stream;
        Ok(Box::pin(stream::iter(vec![Ok(json!({"data": "test"}))])))
    }

    async fn mcp_auth(&self, _server_url: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn mcp_logout(&self, _server_url: Option<&str>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn mcp_auth_status(&self, _server_url: &str) -> anyhow::Result<String> {
        Ok("authenticated".to_string())
    }
}

/// Create a test server with mock API
pub fn create_test_server() -> crate::JsonRpcServer<MockAPI> {
    let api = Arc::new(MockAPI::default());
    crate::JsonRpcServer::new(api)
}

/// Create a test server with custom mock API
pub fn create_test_server_with_mock(mock: MockAPI) -> crate::JsonRpcServer<MockAPI> {
    let api = Arc::new(mock);
    crate::JsonRpcServer::new(api)
}
