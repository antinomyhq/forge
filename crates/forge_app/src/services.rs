use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use derive_setters::Setters;
use forge_domain::{
    AgentId, AnyProvider, Attachment, AuthContextRequest, AuthContextResponse, AuthCredential,
    AuthMethod, ChatCompletionMessage, CommandOutput, Context, Conversation, ConversationId,
    Environment, File, Image, InitAuth, LoginInfo, McpConfig, McpServers, Model, ModelId,
    PatchOperation, Provider, ProviderId, ResultStream, Scope, SecurityCheckResult,
    SecuritySeverity, Template, ToolCallFull, ToolOutput, Workflow, analyze_content_security,
    check_command_security,
};
use merge::Merge;
use reqwest::Response;
use reqwest::header::HeaderMap;
use reqwest_eventsource::EventSource;
use url::Url;

use crate::Walker;
use crate::user::{User, UserUsage};
/// Security validation service for shell commands
pub struct SecurityValidationService;

impl SecurityValidationService {
    /// Create new security validation service
    pub fn new() -> Self {
        Self
    }
}

impl Default for SecurityValidationService {
    fn default() -> Self {
        Self::new()
    }
}

/// Security validation implementation
impl SecurityValidationService {
    /// Check if command is dangerous in restricted mode
    pub fn is_command_blocked(&self, command: &str, restricted: bool) -> anyhow::Result<bool> {
        if !restricted {
            return Ok(false);
        }

        let security_result = check_command_security(command);
        Ok(security_result.is_dangerous)
    }

    /// Get security analysis for command
    pub fn analyze_command(&self, command: &str) -> anyhow::Result<SecurityCheckResult> {
        Ok(check_command_security(command))
    }

    /// Analyze content security for inline shell commands
    pub fn analyze_content(&self, content: &str) -> anyhow::Result<Vec<SecurityCheckResult>> {
        Ok(analyze_content_security(content))
    }

    /// Get reason for blocked command
    pub fn get_block_reason(&self, command: &str) -> anyhow::Result<String> {
        let security_result = check_command_security(command);
        if security_result.is_dangerous {
            Ok(format!(
                "Command blocked in restricted mode: {} (Severity: {:?})",
                security_result.reason, security_result.severity
            ))
        } else {
            Ok("Command is not blocked".to_string())
        }
    }

    /// Check if command severity exceeds threshold
    pub fn exceeds_severity_threshold(
        &self,
        command: &str,
        max_severity: SecuritySeverity,
    ) -> anyhow::Result<bool> {
        let security_result = check_command_security(command);
        Ok(security_result.severity > max_severity)
    }

    /// Get dangerous commands in content above severity threshold
    pub fn get_dangerous_commands_by_severity(
        &self,
        content: &str,
        min_severity: SecuritySeverity,
    ) -> anyhow::Result<Vec<SecurityCheckResult>> {
        let all_dangerous = analyze_content_security(content);
        Ok(all_dangerous
            .into_iter()
            .filter(|result| result.severity >= min_severity)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_security_service() -> SecurityValidationService {
        SecurityValidationService::new()
    }

    #[test]
    fn test_is_command_blocked_restricted_mode() {
        let service = fixture_security_service();

        // Dangerous command should be blocked in restricted mode
        assert!(service.is_command_blocked("rm -rf /", true).unwrap());

        // Safe command should not be blocked
        assert!(!service.is_command_blocked("echo hello", true).unwrap());
    }

    #[test]
    fn test_is_command_blocked_unrestricted_mode() {
        let service = fixture_security_service();

        // Nothing should be blocked in unrestricted mode
        assert!(!service.is_command_blocked("rm -rf /", false).unwrap());
        assert!(!service.is_command_blocked("echo hello", false).unwrap());
    }

    #[test]
    fn test_analyze_command() {
        let service = fixture_security_service();

        let result = service.analyze_command("rm -rf /").unwrap();
        assert!(result.is_dangerous);
        assert_eq!(result.severity, SecuritySeverity::Critical);
        assert!(result.reason.contains("filesystem"));
    }

    #[test]
    fn test_analyze_content() {
        let service = fixture_security_service();

        let content = "Run ![rm -rf /]";
        let results = service.analyze_content(content).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].severity, SecuritySeverity::Critical);
    }

    #[test]
    fn test_get_block_reason() {
        let service = fixture_security_service();

        let reason = service.get_block_reason("rm -rf /").unwrap();
        assert!(reason.contains("Command blocked in restricted mode"));
        assert!(reason.contains("Critical"));

        let safe_reason = service.get_block_reason("echo hello").unwrap();
        assert_eq!(safe_reason, "Command is not blocked");
    }

    #[test]
    fn test_exceeds_severity_threshold() {
        let service = fixture_security_service();

        // Critical command exceeds medium threshold
        assert!(
            service
                .exceeds_severity_threshold("rm -rf /", SecuritySeverity::Medium)
                .unwrap()
        );

        // Low severity command doesn't exceed medium threshold
        assert!(
            !service
                .exceeds_severity_threshold("su root", SecuritySeverity::Medium)
                .unwrap()
        );
    }

    #[test]
    fn test_get_dangerous_commands_by_severity() {
        let service = fixture_security_service();

        let content = "Run ![su root] and ![chmod 777 file] and ![rm -rf /]";

        // Only high and critical severity commands
        let high_and_above = service
            .get_dangerous_commands_by_severity(content, SecuritySeverity::High)
            .unwrap();
        assert_eq!(high_and_above.len(), 2); // chmod 777 (High) and rm -rf / (Critical)

        // Only critical severity commands
        let critical_only = service
            .get_dangerous_commands_by_severity(content, SecuritySeverity::Critical)
            .unwrap();
        assert_eq!(critical_only.len(), 1); // rm -rf / only
    }
}

#[derive(Debug)]
pub struct ShellOutput {
    pub output: CommandOutput,
    pub shell: String,
}

#[derive(Debug)]
pub struct PatchOutput {
    pub warning: Option<String>,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Setters)]
#[setters(into)]
pub struct ReadOutput {
    pub content: Content,
    pub start_line: u64,
    pub end_line: u64,
    pub total_lines: u64,
}

#[derive(Debug)]
pub enum Content {
    File(String),
}

impl Content {
    pub fn file<S: Into<String>>(content: S) -> Self {
        Self::File(content.into())
    }

    pub fn file_content(&self) -> &str {
        match self {
            Self::File(content) => content,
        }
    }
}

#[derive(Debug)]
pub struct SearchResult {
    pub matches: Vec<Match>,
}

#[derive(Debug)]
pub struct Match {
    pub path: String,
    pub result: Option<MatchResult>,
}

#[derive(Debug)]
pub enum MatchResult {
    Error(String),
    Found { line_number: usize, line: String },
}

#[derive(Debug)]
pub struct HttpResponse {
    pub content: String,
    pub code: u16,
    pub context: ResponseContext,
    pub content_type: String,
}

#[derive(Debug)]
pub enum ResponseContext {
    Parsed,
    Raw,
}

#[derive(Debug)]
pub struct FsCreateOutput {
    pub path: String,
    // Set when the file already exists
    pub before: Option<String>,
    pub warning: Option<String>,
}

#[derive(Debug)]
pub struct FsRemoveOutput {
    // Content of the file
    pub content: String,
}

#[derive(Debug)]
pub struct PlanCreateOutput {
    pub path: PathBuf,
    // Set when the file already exists
    pub before: Option<String>,
}

#[derive(Default, Debug, derive_more::From)]
pub struct FsUndoOutput {
    pub before_undo: Option<String>,
    pub after_undo: Option<String>,
}

#[derive(Debug)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub path: Option<PathBuf>,
}

#[async_trait::async_trait]
pub trait ProviderService: Send + Sync {
    async fn chat(
        &self,
        id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error>;
    async fn models(&self, provider: Provider<Url>) -> anyhow::Result<Vec<Model>>;
    async fn get_provider(&self, id: forge_domain::ProviderId) -> anyhow::Result<Provider<Url>>;
    async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>>;
    async fn upsert_credential(
        &self,
        credential: forge_domain::AuthCredential,
    ) -> anyhow::Result<()>;
    async fn remove_credential(&self, id: &forge_domain::ProviderId) -> anyhow::Result<()>;
    /// Migrate env-based credentials to file-based credentials
    async fn migrate_env_credentials(
        &self,
    ) -> anyhow::Result<Option<forge_domain::MigrationResult>>;
}

/// Manage user preferences for default providers and models.
#[async_trait::async_trait]
pub trait AppConfigService: Send + Sync {
    /// Get user's default provider or fallback to first available
    async fn get_default_provider(&self) -> anyhow::Result<Provider<Url>>;

    /// Set user's default provider preference
    async fn set_default_provider(
        &self,
        provider_id: forge_domain::ProviderId,
    ) -> anyhow::Result<()>;

    /// Get user's default model for specific provider
    async fn get_default_model(
        &self,
        provider_id: &forge_domain::ProviderId,
    ) -> anyhow::Result<ModelId>;

    /// Set user's default model for specific provider
    async fn set_default_model(
        &self,
        model: ModelId,
        provider_id: forge_domain::ProviderId,
    ) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait McpConfigManager: Send + Sync {
    /// Load MCP servers from configuration files
    async fn read_mcp_config(&self, scope: Option<&Scope>) -> anyhow::Result<McpConfig>;

    /// Write McpConfig to disk
    async fn write_mcp_config(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait McpService: Send + Sync {
    async fn get_mcp_servers(&self) -> anyhow::Result<McpServers>;
    async fn execute_mcp(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput>;
    /// Refresh MCP cache by fetching fresh data
    async fn reload_mcp(&self) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait ConversationService: Send + Sync {
    async fn find_conversation(&self, id: &ConversationId) -> anyhow::Result<Option<Conversation>>;

    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()>;

    /// Perform atomic operations on conversation
    async fn modify_conversation<F, T>(&self, id: &ConversationId, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send,
        T: Send;

    /// Find conversations with optional limit
    async fn get_conversations(
        &self,
        limit: Option<usize>,
    ) -> anyhow::Result<Option<Vec<Conversation>>>;

    /// Find last active conversation
    async fn last_conversation(&self) -> anyhow::Result<Option<Conversation>>;
}

#[async_trait::async_trait]
pub trait TemplateService: Send + Sync {
    async fn register_template(&self, path: PathBuf) -> anyhow::Result<()>;
    async fn render_template<V: serde::Serialize + Send + Sync>(
        &self,
        template: Template<V>,
        object: &V,
    ) -> anyhow::Result<String>;
}

#[async_trait::async_trait]
pub trait AttachmentService {
    async fn attachments(&self, url: &str) -> anyhow::Result<Vec<Attachment>>;
}

pub trait EnvironmentService: Send + Sync {
    fn get_environment(&self) -> Environment;
}
#[async_trait::async_trait]
pub trait CustomInstructionsService: Send + Sync {
    async fn get_custom_instructions(&self) -> Vec<String>;
}

#[async_trait::async_trait]
pub trait WorkflowService {
    /// Find forge.yaml config file by traversing parent directories
    async fn resolve(&self, path: Option<std::path::PathBuf>) -> std::path::PathBuf;

    /// Read workflow from given path or find forge.yaml
    async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow>;

    /// Read workflow and merge with default workflow
    async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let workflow = self.read_workflow(path).await?;
        let mut base_workflow = Workflow::default();
        base_workflow.merge(workflow);
        Ok(base_workflow)
    }

    /// Write workflow to specified path or find forge.yaml
    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()>;

    /// Update workflow using closure at given path
    async fn update_workflow<F>(&self, path: Option<&Path>, f: F) -> anyhow::Result<Workflow>
    where
        F: FnOnce(&mut Workflow) + Send;
}

#[async_trait::async_trait]
pub trait FileDiscoveryService: Send + Sync {
    async fn collect_files(&self, config: Walker) -> anyhow::Result<Vec<File>>;
}

#[async_trait::async_trait]
pub trait FsCreateService: Send + Sync {
    /// Create file at specified path with given content
    async fn create(
        &self,
        path: String,
        content: String,
        overwrite: bool,
    ) -> anyhow::Result<FsCreateOutput>;
}

#[async_trait::async_trait]
pub trait PlanCreateService: Send + Sync {
    /// Create plan file with specified name and version
    async fn create_plan(
        &self,
        plan_name: String,
        version: String,
        content: String,
    ) -> anyhow::Result<PlanCreateOutput>;
}

#[async_trait::async_trait]
pub trait FsPatchService: Send + Sync {
    /// Patch file at specified path with given content
    async fn patch(
        &self,
        path: String,
        search: Option<String>,
        operation: PatchOperation,
        content: String,
    ) -> anyhow::Result<PatchOutput>;
}

#[async_trait::async_trait]
pub trait FsReadService: Send + Sync {
    /// Read file at specified path and return content
    async fn read(
        &self,
        path: String,
        start_line: Option<u64>,
        end_line: Option<u64>,
    ) -> anyhow::Result<ReadOutput>;
}

#[async_trait::async_trait]
pub trait ImageReadService: Send + Sync {
    /// Read image file at specified path and return content
    async fn read_image(&self, path: String) -> anyhow::Result<forge_domain::Image>;
}

#[async_trait::async_trait]
pub trait FsRemoveService: Send + Sync {
    /// Remove file at specified path
    async fn remove(&self, path: String) -> anyhow::Result<FsRemoveOutput>;
}

#[async_trait::async_trait]
pub trait FsSearchService: Send + Sync {
    /// Search for file at specified path and return content
    async fn search(
        &self,
        path: String,
        regex: Option<String>,
        file_pattern: Option<String>,
    ) -> anyhow::Result<Option<SearchResult>>;
}

#[async_trait::async_trait]
pub trait FollowUpService: Send + Sync {
    /// Follow up on tool call with given context
    async fn follow_up(
        &self,
        question: String,
        options: Vec<String>,
        multiple: Option<bool>,
    ) -> anyhow::Result<Option<String>>;
}

#[async_trait::async_trait]
pub trait FsUndoService: Send + Sync {
    /// Undo last file operation at specified path
    async fn undo(&self, path: String) -> anyhow::Result<FsUndoOutput>;
}

#[async_trait::async_trait]
pub trait NetFetchService: Send + Sync {
    /// Fetch content from URL and return as string
    async fn fetch(&self, url: String, raw: Option<bool>) -> anyhow::Result<HttpResponse>;
}

#[async_trait::async_trait]
pub trait ShellService: Send + Sync {
    /// Execute shell command and return output
    async fn execute(
        &self,
        command: String,
        cwd: PathBuf,
        keep_ansi: bool,
        silent: bool,
        env_vars: Option<Vec<String>>,
    ) -> anyhow::Result<ShellOutput>;
}

#[async_trait::async_trait]
pub trait AuthService: Send + Sync {
    async fn init_auth(&self) -> anyhow::Result<InitAuth>;
    async fn login(&self, auth: &InitAuth) -> anyhow::Result<LoginInfo>;
    async fn user_info(&self, api_key: &str) -> anyhow::Result<User>;
    async fn user_usage(&self, api_key: &str) -> anyhow::Result<UserUsage>;
    async fn get_auth_token(&self) -> anyhow::Result<Option<LoginInfo>>;
    async fn set_auth_token(&self, token: Option<LoginInfo>) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait AgentRegistry: Send + Sync {
    /// Get active agent ID
    async fn get_active_agent_id(&self) -> anyhow::Result<Option<AgentId>>;

    /// Set active agent ID
    async fn set_active_agent_id(&self, agent_id: AgentId) -> anyhow::Result<()>;

    /// Get all agents from registry store
    async fn get_agents(&self) -> anyhow::Result<Vec<forge_domain::Agent>>;

    /// Get agent by ID from registry store
    async fn get_agent(&self, agent_id: &AgentId) -> anyhow::Result<Option<forge_domain::Agent>>;

    /// Reload agents by invalidating cache
    async fn reload_agents(&self) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait CommandLoaderService: Send + Sync {
    /// Load command definitions from forge/commands directory
    async fn get_commands(&self) -> anyhow::Result<Vec<forge_domain::Command>>;
}

#[async_trait::async_trait]
pub trait PolicyService: Send + Sync {
    /// Check operation permission and handle user confirmation
    async fn check_operation_permission(
        &self,
        operation: &forge_domain::PermissionOperation,
    ) -> anyhow::Result<PolicyDecision>;
}

/// Provider authentication service
#[async_trait::async_trait]
pub trait ProviderAuthService: Send + Sync {
    async fn init_provider_auth(
        &self,
        provider_id: ProviderId,
        method: AuthMethod,
    ) -> anyhow::Result<AuthContextRequest>;
    async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        context: AuthContextResponse,
        timeout: Duration,
    ) -> anyhow::Result<()>;
    async fn refresh_provider_credential(
        &self,
        provider: &Provider<Url>,
        method: AuthMethod,
    ) -> anyhow::Result<AuthCredential>;
}

/// Core app trait providing access to services and repositories
pub trait Services: Send + Sync + 'static + Clone {
    type ProviderService: ProviderService;
    type AppConfigService: AppConfigService;
    type ConversationService: ConversationService;
    type TemplateService: TemplateService;
    type AttachmentService: AttachmentService;
    type EnvironmentService: EnvironmentService;
    type CustomInstructionsService: CustomInstructionsService;
    type WorkflowService: WorkflowService + Sync;
    type FileDiscoveryService: FileDiscoveryService;
    type McpConfigManager: McpConfigManager;
    type FsCreateService: FsCreateService;
    type PlanCreateService: PlanCreateService;
    type FsPatchService: FsPatchService;
    type FsReadService: FsReadService;
    type ImageReadService: ImageReadService;
    type FsRemoveService: FsRemoveService;
    type FsSearchService: FsSearchService;
    type FollowUpService: FollowUpService;
    type FsUndoService: FsUndoService;
    type NetFetchService: NetFetchService;
    type ShellService: ShellService;
    type McpService: McpService;
    type AuthService: AuthService;
    type AgentRegistry: AgentRegistry;
    type CommandLoaderService: CommandLoaderService;
    type PolicyService: PolicyService;
    type ProviderAuthService: ProviderAuthService;
    type InlineShellExecutor: crate::inline_shell::InlineShellExecutor;
    type PromptProcessor: crate::services::prompt_processor::PromptProcessor;

    fn provider_service(&self) -> &Self::ProviderService;
    fn config_service(&self) -> &Self::AppConfigService;
    fn conversation_service(&self) -> &Self::ConversationService;
    fn template_service(&self) -> &Self::TemplateService;
    fn attachment_service(&self) -> &Self::AttachmentService;
    fn workflow_service(&self) -> &Self::WorkflowService;
    fn file_discovery_service(&self) -> &Self::FileDiscoveryService;
    fn mcp_config_manager(&self) -> &Self::McpConfigManager;
    fn fs_create_service(&self) -> &Self::FsCreateService;
    fn plan_create_service(&self) -> &Self::PlanCreateService;
    fn fs_patch_service(&self) -> &Self::FsPatchService;
    fn fs_read_service(&self) -> &Self::FsReadService;
    fn image_read_service(&self) -> &Self::ImageReadService;
    fn fs_remove_service(&self) -> &Self::FsRemoveService;
    fn fs_search_service(&self) -> &Self::FsSearchService;
    fn follow_up_service(&self) -> &Self::FollowUpService;
    fn fs_undo_service(&self) -> &Self::FsUndoService;
    fn net_fetch_service(&self) -> &Self::NetFetchService;
    fn shell_service(&self) -> &Self::ShellService;
    fn mcp_service(&self) -> &Self::McpService;
    fn environment_service(&self) -> &Self::EnvironmentService;
    fn custom_instructions_service(&self) -> &Self::CustomInstructionsService;
    fn auth_service(&self) -> &Self::AuthService;
    fn agent_registry(&self) -> &Self::AgentRegistry;
    fn command_loader_service(&self) -> &Self::CommandLoaderService;
    fn policy_service(&self) -> &Self::PolicyService;
    fn provider_auth_service(&self) -> &Self::ProviderAuthService;
    fn inline_shell_executor(&self) -> Arc<Self::InlineShellExecutor>;
    fn prompt_processor(&self) -> Arc<Self::PromptProcessor>;
}

#[async_trait::async_trait]
impl<I: Services> ConversationService for I {
    async fn find_conversation(&self, id: &ConversationId) -> anyhow::Result<Option<Conversation>> {
        self.conversation_service().find_conversation(id).await
    }

    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.conversation_service()
            .upsert_conversation(conversation)
            .await
    }

    async fn modify_conversation<F, T>(&self, id: &ConversationId, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send,
        T: Send,
    {
        self.conversation_service().modify_conversation(id, f).await
    }

    async fn get_conversations(
        &self,
        limit: Option<usize>,
    ) -> anyhow::Result<Option<Vec<Conversation>>> {
        self.conversation_service().get_conversations(limit).await
    }

    async fn last_conversation(&self) -> anyhow::Result<Option<Conversation>> {
        self.conversation_service().last_conversation().await
    }
}
#[async_trait::async_trait]
impl<I: Services> ProviderService for I {
    async fn chat(
        &self,
        id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.provider_service().chat(id, context, provider).await
    }

    async fn models(&self, provider: Provider<Url>) -> anyhow::Result<Vec<Model>> {
        self.provider_service().models(provider).await
    }

    async fn get_provider(&self, id: forge_domain::ProviderId) -> anyhow::Result<Provider<Url>> {
        self.provider_service().get_provider(id).await
    }

    async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>> {
        self.provider_service().get_all_providers().await
    }

    async fn upsert_credential(
        &self,
        credential: forge_domain::AuthCredential,
    ) -> anyhow::Result<()> {
        self.provider_service().upsert_credential(credential).await
    }

    async fn remove_credential(&self, id: &forge_domain::ProviderId) -> anyhow::Result<()> {
        self.provider_service().remove_credential(id).await
    }

    async fn migrate_env_credentials(
        &self,
    ) -> anyhow::Result<Option<forge_domain::MigrationResult>> {
        self.provider_service().migrate_env_credentials().await
    }
}

#[async_trait::async_trait]
impl<I: Services> McpConfigManager for I {
    async fn read_mcp_config(&self, scope: Option<&Scope>) -> anyhow::Result<McpConfig> {
        self.mcp_config_manager().read_mcp_config(scope).await
    }

    async fn write_mcp_config(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()> {
        self.mcp_config_manager()
            .write_mcp_config(config, scope)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> McpService for I {
    async fn get_mcp_servers(&self) -> anyhow::Result<McpServers> {
        self.mcp_service().get_mcp_servers().await
    }

    async fn execute_mcp(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput> {
        self.mcp_service().execute_mcp(call).await
    }

    async fn reload_mcp(&self) -> anyhow::Result<()> {
        self.mcp_service().reload_mcp().await
    }
}

#[async_trait::async_trait]
impl<I: Services> TemplateService for I {
    async fn register_template(&self, path: PathBuf) -> anyhow::Result<()> {
        self.template_service().register_template(path).await
    }

    async fn render_template<V: serde::Serialize + Send + Sync>(
        &self,
        template: Template<V>,
        object: &V,
    ) -> anyhow::Result<String> {
        self.template_service()
            .render_template(template, object)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> AttachmentService for I {
    async fn attachments(&self, url: &str) -> anyhow::Result<Vec<Attachment>> {
        self.attachment_service().attachments(url).await
    }
}

#[async_trait::async_trait]
impl<I: Services> WorkflowService for I {
    async fn resolve(&self, path: Option<std::path::PathBuf>) -> std::path::PathBuf {
        self.workflow_service().resolve(path).await
    }

    async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        self.workflow_service().read_workflow(path).await
    }

    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()> {
        self.workflow_service().write_workflow(path, workflow).await
    }

    async fn update_workflow<F>(&self, path: Option<&Path>, f: F) -> anyhow::Result<Workflow>
    where
        F: FnOnce(&mut Workflow) + Send,
    {
        self.workflow_service().update_workflow(path, f).await
    }
}

#[async_trait::async_trait]
impl<I: Services> FileDiscoveryService for I {
    async fn collect_files(&self, config: Walker) -> anyhow::Result<Vec<File>> {
        self.file_discovery_service().collect_files(config).await
    }
}

#[async_trait::async_trait]
impl<I: Services> FsCreateService for I {
    async fn create(
        &self,
        path: String,
        content: String,
        overwrite: bool,
    ) -> anyhow::Result<FsCreateOutput> {
        self.fs_create_service()
            .create(path, content, overwrite)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> PlanCreateService for I {
    async fn create_plan(
        &self,
        plan_name: String,
        version: String,
        content: String,
    ) -> anyhow::Result<PlanCreateOutput> {
        self.plan_create_service()
            .create_plan(plan_name, version, content)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> FsPatchService for I {
    async fn patch(
        &self,
        path: String,
        search: Option<String>,
        operation: PatchOperation,
        content: String,
    ) -> anyhow::Result<PatchOutput> {
        self.fs_patch_service()
            .patch(path, search, operation, content)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> FsReadService for I {
    async fn read(
        &self,
        path: String,
        start_line: Option<u64>,
        end_line: Option<u64>,
    ) -> anyhow::Result<ReadOutput> {
        self.fs_read_service()
            .read(path, start_line, end_line)
            .await
    }
}
#[async_trait::async_trait]
impl<I: Services> ImageReadService for I {
    async fn read_image(&self, path: String) -> anyhow::Result<Image> {
        self.image_read_service().read_image(path).await
    }
}

#[async_trait::async_trait]
impl<I: Services> FsRemoveService for I {
    async fn remove(&self, path: String) -> anyhow::Result<FsRemoveOutput> {
        self.fs_remove_service().remove(path).await
    }
}

#[async_trait::async_trait]
impl<I: Services> FsSearchService for I {
    async fn search(
        &self,
        path: String,
        regex: Option<String>,
        file_pattern: Option<String>,
    ) -> anyhow::Result<Option<SearchResult>> {
        self.fs_search_service()
            .search(path, regex, file_pattern)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> FollowUpService for I {
    async fn follow_up(
        &self,
        question: String,
        options: Vec<String>,
        multiple: Option<bool>,
    ) -> anyhow::Result<Option<String>> {
        self.follow_up_service()
            .follow_up(question, options, multiple)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> FsUndoService for I {
    async fn undo(&self, path: String) -> anyhow::Result<FsUndoOutput> {
        self.fs_undo_service().undo(path).await
    }
}

#[async_trait::async_trait]
impl<I: Services> NetFetchService for I {
    async fn fetch(&self, url: String, raw: Option<bool>) -> anyhow::Result<HttpResponse> {
        self.net_fetch_service().fetch(url, raw).await
    }
}

#[async_trait::async_trait]
impl<I: Services> ShellService for I {
    async fn execute(
        &self,
        command: String,
        cwd: PathBuf,
        keep_ansi: bool,
        silent: bool,
        env_vars: Option<Vec<String>>,
    ) -> anyhow::Result<ShellOutput> {
        self.shell_service()
            .execute(command, cwd, keep_ansi, silent, env_vars)
            .await
    }
}

impl<I: Services> EnvironmentService for I {
    fn get_environment(&self) -> Environment {
        self.environment_service().get_environment()
    }
}

#[async_trait::async_trait]
impl<I: Services> CustomInstructionsService for I {
    async fn get_custom_instructions(&self) -> Vec<String> {
        self.custom_instructions_service()
            .get_custom_instructions()
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> AuthService for I {
    async fn init_auth(&self) -> anyhow::Result<InitAuth> {
        self.auth_service().init_auth().await
    }

    async fn login(&self, auth: &InitAuth) -> anyhow::Result<LoginInfo> {
        self.auth_service().login(auth).await
    }

    async fn user_info(&self, api_key: &str) -> anyhow::Result<User> {
        self.auth_service().user_info(api_key).await
    }

    async fn user_usage(&self, api_key: &str) -> anyhow::Result<UserUsage> {
        self.auth_service().user_usage(api_key).await
    }

    async fn get_auth_token(&self) -> anyhow::Result<Option<LoginInfo>> {
        self.auth_service().get_auth_token().await
    }

    async fn set_auth_token(&self, token: Option<LoginInfo>) -> anyhow::Result<()> {
        self.auth_service().set_auth_token(token).await
    }
}

/// HTTP service trait for making requests
#[async_trait::async_trait]
pub trait HttpClientService: Send + Sync + 'static {
    async fn get(&self, url: &Url, headers: Option<HeaderMap>) -> anyhow::Result<Response>;
    async fn post(&self, url: &Url, body: bytes::Bytes) -> anyhow::Result<Response>;
    async fn delete(&self, url: &Url) -> anyhow::Result<Response>;

    /// Post JSON data and return server-sent events stream
    async fn eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<EventSource>;
}

#[async_trait::async_trait]
impl<I: Services> AgentRegistry for I {
    async fn get_active_agent_id(&self) -> anyhow::Result<Option<AgentId>> {
        self.agent_registry().get_active_agent_id().await
    }

    async fn set_active_agent_id(&self, agent_id: AgentId) -> anyhow::Result<()> {
        self.agent_registry().set_active_agent_id(agent_id).await
    }

    async fn get_agents(&self) -> anyhow::Result<Vec<forge_domain::Agent>> {
        self.agent_registry().get_agents().await
    }

    async fn get_agent(&self, agent_id: &AgentId) -> anyhow::Result<Option<forge_domain::Agent>> {
        self.agent_registry().get_agent(agent_id).await
    }

    async fn reload_agents(&self) -> anyhow::Result<()> {
        self.agent_registry().reload_agents().await
    }
}

#[async_trait::async_trait]
impl<I: Services> CommandLoaderService for I {
    async fn get_commands(&self) -> anyhow::Result<Vec<forge_domain::Command>> {
        self.command_loader_service().get_commands().await
    }
}

#[async_trait::async_trait]
impl<I: Services> PolicyService for I {
    async fn check_operation_permission(
        &self,
        operation: &forge_domain::PermissionOperation,
    ) -> anyhow::Result<PolicyDecision> {
        self.policy_service()
            .check_operation_permission(operation)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> AppConfigService for I {
    async fn get_default_provider(&self) -> anyhow::Result<Provider<Url>> {
        self.config_service().get_default_provider().await
    }

    async fn set_default_provider(
        &self,
        provider_id: forge_domain::ProviderId,
    ) -> anyhow::Result<()> {
        self.config_service()
            .set_default_provider(provider_id)
            .await
    }

    async fn get_default_model(
        &self,
        provider_id: &forge_domain::ProviderId,
    ) -> anyhow::Result<ModelId> {
        self.config_service().get_default_model(provider_id).await
    }

    async fn set_default_model(
        &self,
        model: ModelId,
        provider_id: forge_domain::ProviderId,
    ) -> anyhow::Result<()> {
        self.config_service()
            .set_default_model(model, provider_id)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> ProviderAuthService for I {
    async fn init_provider_auth(
        &self,
        provider_id: ProviderId,
        method: AuthMethod,
    ) -> anyhow::Result<AuthContextRequest> {
        self.provider_auth_service()
            .init_provider_auth(provider_id, method)
            .await
    }
    async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        context: AuthContextResponse,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        self.provider_auth_service()
            .complete_provider_auth(provider_id, context, timeout)
            .await
    }
    async fn refresh_provider_credential(
        &self,
        provider: &Provider<Url>,
        method: AuthMethod,
    ) -> anyhow::Result<AuthCredential> {
        self.provider_auth_service()
            .refresh_provider_credential(provider, method)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> crate::inline_shell::InlineShellExecutor for I {
    async fn execute_commands(
        &self,
        commands: Vec<forge_domain::inline_shell::InlineShellCommand>,
        working_dir: &std::path::Path,
        restricted: bool,
    ) -> Result<Vec<forge_domain::CommandResult>, forge_domain::inline_shell::InlineShellError>
    {
        self.inline_shell_executor()
            .execute_commands(commands, working_dir, restricted)
            .await
    }
}

// Security context and prompt processor modules
pub mod mock_inline_shell_executor;
pub mod prompt_processor;
pub mod security_context;

pub use mock_inline_shell_executor::*;
pub use prompt_processor::PromptProcessor;
pub use security_context::*;
