use std::path::{Path, PathBuf};

use bytes::Bytes;
use forge_domain::{
    Agent, Attachment, ChatCompletionMessage, CommandOutput, Context, Conversation, ConversationId,
    Environment, File, McpConfig, Model, ModelId, PatchOperation, Provider, ResultStream, Scope,
    ToolCallFull, ToolDefinition, ToolOutput, Workflow,
};
use merge::Merge;
use reqwest::Response;
use reqwest::header::HeaderMap;
use reqwest_eventsource::EventSource;
use url::Url;

use crate::Walker;
use crate::dto::{AppConfig, InitAuth, LoginInfo};
use crate::user::{User, UserUsage};

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

#[derive(Debug)]
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
        provider: Provider,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error>;
    async fn models(&self, provider: Provider) -> anyhow::Result<Vec<Model>>;
}

#[async_trait::async_trait]
pub trait McpConfigManager: Send + Sync {
    /// Responsible to load the MCP servers from all configuration files.
    async fn read_mcp_config(&self) -> anyhow::Result<McpConfig>;

    /// Responsible for writing the McpConfig on disk.
    async fn write_mcp_config(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait McpService: Send + Sync {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>>;
    async fn call(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput>;
}

#[async_trait::async_trait]
pub trait ConversationService: Send + Sync {
    async fn find(&self, id: &ConversationId) -> anyhow::Result<Option<Conversation>>;

    async fn upsert(&self, conversation: Conversation) -> anyhow::Result<()>;

    async fn create_conversation(&self, workflow: Workflow) -> anyhow::Result<Conversation>;

    /// This is useful when you want to perform several operations on a
    /// conversation atomically.
    async fn update<F, T>(&self, id: &ConversationId, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send;
}

#[async_trait::async_trait]
pub trait TemplateService: Send + Sync {
    async fn register_template(&self, path: PathBuf) -> anyhow::Result<()>;
    async fn render_template(
        &self,
        template: impl ToString + Send,
        object: &(impl serde::Serialize + Sync),
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
    /// Find a forge.yaml config file by traversing parent directories.
    /// Returns the path to the first found config file, or the original path if
    /// none is found.
    async fn resolve(&self, path: Option<std::path::PathBuf>) -> std::path::PathBuf;

    /// Reads the workflow from the given path.
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories.
    async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow>;

    /// Reads the workflow from the given path and merges it with an default
    /// workflow.
    async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let workflow = self.read_workflow(path).await?;
        let mut base_workflow = Workflow::default();
        base_workflow.merge(workflow);
        Ok(base_workflow)
    }

    /// Writes the given workflow to the specified path.
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories.
    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()>;

    /// Updates the workflow at the given path using the provided closure.
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories.
    ///
    /// The closure receives a mutable reference to the workflow, which can be
    /// modified. After the closure completes, the updated workflow is
    /// written back to the same path.
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
    /// Create a file at the specified path with the given content.
    async fn create(
        &self,
        path: String,
        content: String,
        overwrite: bool,
        capture_snapshot: bool,
    ) -> anyhow::Result<FsCreateOutput>;
}

#[async_trait::async_trait]
pub trait PlanCreateService: Send + Sync {
    /// Create a plan file with the specified name and version.
    async fn create_plan(
        &self,
        plan_name: String,
        version: String,
        content: String,
    ) -> anyhow::Result<PlanCreateOutput>;
}

#[async_trait::async_trait]
pub trait FsPatchService: Send + Sync {
    /// Patches a file at the specified path with the given content.
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
    /// Reads a file at the specified path and returns its content.
    async fn read(
        &self,
        path: String,
        start_line: Option<u64>,
        end_line: Option<u64>,
    ) -> anyhow::Result<ReadOutput>;
}

#[async_trait::async_trait]
pub trait FsRemoveService: Send + Sync {
    /// Removes a file at the specified path.
    async fn remove(&self, path: String) -> anyhow::Result<FsRemoveOutput>;
}

#[async_trait::async_trait]
pub trait FsSearchService: Send + Sync {
    /// Searches for a file at the specified path and returns its content.
    async fn search(
        &self,
        path: String,
        regex: Option<String>,
        file_pattern: Option<String>,
    ) -> anyhow::Result<Option<SearchResult>>;
}

#[async_trait::async_trait]
pub trait FollowUpService: Send + Sync {
    /// Follows up on a tool call with the given context.
    async fn follow_up(
        &self,
        question: String,
        options: Vec<String>,
        multiple: Option<bool>,
    ) -> anyhow::Result<Option<String>>;
}

#[async_trait::async_trait]
pub trait FsUndoService: Send + Sync {
    /// Undoes the last file operation at the specified path.
    /// And returns the content of the undone file.
    // TODO: We should move Snapshot service to Services from infra
    // and drop FsUndoService.
    async fn undo(&self, path: String) -> anyhow::Result<FsUndoOutput>;
}

#[async_trait::async_trait]
pub trait NetFetchService: Send + Sync {
    /// Fetches content from a URL and returns it as a string.
    async fn fetch(&self, url: String, raw: Option<bool>) -> anyhow::Result<HttpResponse>;
}

#[async_trait::async_trait]
pub trait ShellService: Send + Sync {
    /// Executes a shell command and returns the output.
    async fn execute(
        &self,
        command: String,
        cwd: PathBuf,
        keep_ansi: bool,
    ) -> anyhow::Result<ShellOutput>;
}

#[async_trait::async_trait]
pub trait AppConfigService: Send + Sync {
    async fn read_app_config(&self) -> anyhow::Result<AppConfig>;
    async fn write_app_config(&self, config: &AppConfig) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait AuthService: Send + Sync {
    async fn init_auth(&self) -> anyhow::Result<InitAuth>;
    async fn login(&self, auth: &InitAuth) -> anyhow::Result<LoginInfo>;
    async fn user_info(&self, api_key: &str) -> anyhow::Result<User>;
    async fn user_usage(&self, api_key: &str) -> anyhow::Result<UserUsage>;
}
#[async_trait::async_trait]
pub trait ProviderRegistry: Send + Sync {
    async fn get_provider(&self, config: AppConfig) -> anyhow::Result<Provider>;
}

#[async_trait::async_trait]
pub trait AgentLoaderService: Send + Sync {
    /// Load all agent definitions from the forge/agent directory
    async fn load_agents(&self) -> anyhow::Result<Vec<Agent>>;
}

#[async_trait::async_trait]
pub trait PolicyService: Send + Sync {
    /// Check if an operation is allowed and handle user confirmation if needed
    /// Returns PolicyDecision with allowed flag and optional policy file path
    /// (only when created)
    async fn check_operation_permission(
        &self,
        operation: &forge_domain::PermissionOperation,
    ) -> anyhow::Result<PolicyDecision>;
}

/// Core app trait providing access to services and repositories.
/// This trait follows clean architecture principles for dependency management
/// and service/repository composition.
pub trait Services: Send + Sync + 'static + Clone {
    type ProviderService: ProviderService;
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
    type FsRemoveService: FsRemoveService;
    type FsSearchService: FsSearchService;
    type FollowUpService: FollowUpService;
    type FsUndoService: FsUndoService;
    type NetFetchService: NetFetchService;
    type ShellService: ShellService;
    type McpService: McpService;
    type AuthService: AuthService;
    type AppConfigService: AppConfigService;
    type ProviderRegistry: ProviderRegistry;
    type AgentLoaderService: AgentLoaderService;
    type PolicyService: PolicyService;

    fn provider_service(&self) -> &Self::ProviderService;
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
    fn app_config_service(&self) -> &Self::AppConfigService;
    fn provider_registry(&self) -> &Self::ProviderRegistry;
    fn agent_loader_service(&self) -> &Self::AgentLoaderService;
    fn policy_service(&self) -> &Self::PolicyService;
}

#[async_trait::async_trait]
impl<I: Services> ConversationService for I {
    async fn find(&self, id: &ConversationId) -> anyhow::Result<Option<Conversation>> {
        self.conversation_service().find(id).await
    }

    async fn upsert(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.conversation_service().upsert(conversation).await
    }

    async fn create_conversation(&self, workflow: Workflow) -> anyhow::Result<Conversation> {
        self.conversation_service()
            .create_conversation(workflow)
            .await
    }

    async fn update<F, T>(&self, id: &ConversationId, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send,
    {
        self.conversation_service().update(id, f).await
    }
}
#[async_trait::async_trait]
impl<I: Services> ProviderService for I {
    async fn chat(
        &self,
        id: &ModelId,
        context: Context,
        provider: Provider,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.provider_service().chat(id, context, provider).await
    }

    async fn models(&self, provider: Provider) -> anyhow::Result<Vec<Model>> {
        self.provider_service().models(provider).await
    }
}

#[async_trait::async_trait]
impl<I: Services> McpConfigManager for I {
    async fn read_mcp_config(&self) -> anyhow::Result<McpConfig> {
        self.mcp_config_manager().read_mcp_config().await
    }

    async fn write_mcp_config(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()> {
        self.mcp_config_manager()
            .write_mcp_config(config, scope)
            .await
    }
}

#[async_trait::async_trait]
impl<I: Services> McpService for I {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.mcp_service().list().await
    }

    async fn call(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput> {
        self.mcp_service().call(call).await
    }
}

#[async_trait::async_trait]
impl<I: Services> TemplateService for I {
    async fn register_template(&self, path: PathBuf) -> anyhow::Result<()> {
        self.template_service().register_template(path).await
    }

    async fn render_template(
        &self,
        template: impl ToString + Send,
        object: &(impl serde::Serialize + Sync),
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
        capture_snapshot: bool,
    ) -> anyhow::Result<FsCreateOutput> {
        self.fs_create_service()
            .create(path, content, overwrite, capture_snapshot)
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
    ) -> anyhow::Result<ShellOutput> {
        self.shell_service().execute(command, cwd, keep_ansi).await
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
impl<I: Services> ProviderRegistry for I {
    async fn get_provider(&self, config: AppConfig) -> anyhow::Result<Provider> {
        self.provider_registry().get_provider(config).await
    }
}

#[async_trait::async_trait]
impl<I: Services> AppConfigService for I {
    async fn read_app_config(&self) -> anyhow::Result<AppConfig> {
        self.app_config_service().read_app_config().await
    }

    async fn write_app_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        self.app_config_service().write_app_config(config).await
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
}

/// HTTP service trait for making HTTP requests
#[async_trait::async_trait]
pub trait HttpClientService: Send + Sync + 'static {
    async fn get(&self, url: &Url, headers: Option<HeaderMap>) -> anyhow::Result<Response>;
    async fn post(&self, url: &Url, body: bytes::Bytes) -> anyhow::Result<Response>;
    async fn delete(&self, url: &Url) -> anyhow::Result<Response>;

    /// Posts JSON data and returns a server-sent events stream
    async fn eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<EventSource>;
}

#[async_trait::async_trait]
impl<I: Services> AgentLoaderService for I {
    async fn load_agents(&self) -> anyhow::Result<Vec<Agent>> {
        self.agent_loader_service().load_agents().await
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
