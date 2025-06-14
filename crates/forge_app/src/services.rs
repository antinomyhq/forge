use std::path::{Path, PathBuf};

use forge_domain::{
    Attachment, ChatCompletionMessage, CommandOutput, Context, Conversation, ConversationId,
    Environment, File, McpConfig, Model, ModelId, PatchOperation, ResultStream, Scope,
    ToolCallFull, ToolDefinition, ToolOutput, Workflow,
};
use merge::Merge;

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
    pub previous: Option<String>,
    pub warning: Option<String>,
}

#[derive(Debug)]
pub struct FsRemoveOutput {}

#[derive(Debug, derive_more::From)]
pub struct FsUndoOutput {
    pub before_undo: String,
    pub after_undo: String,
}

#[async_trait::async_trait]
pub trait ProviderService: Send + Sync + 'static {
    async fn chat(
        &self,
        id: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error>;
    async fn models(&self) -> anyhow::Result<Vec<Model>>;
}

#[async_trait::async_trait]
pub trait McpConfigManager: Send + Sync {
    /// Responsible to load the MCP servers from all configuration files.
    async fn read(&self) -> anyhow::Result<McpConfig>;

    /// Responsible for writing the McpConfig on disk.
    async fn write(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()>;
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

    async fn create(&self, workflow: Workflow) -> anyhow::Result<Conversation>;

    /// This is useful when you want to perform several operations on a
    /// conversation atomically.
    async fn update<F, T>(&self, id: &ConversationId, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send;
}

#[async_trait::async_trait]
pub trait TemplateService: Send + Sync {
    async fn register_template(&self, path: String) -> anyhow::Result<()>;
    async fn render(
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
pub trait WorkflowService {
    /// Find a forge.yaml config file by traversing parent directories.
    /// Returns the path to the first found config file, or the original path if
    /// none is found.
    async fn resolve(&self, path: Option<std::path::PathBuf>) -> std::path::PathBuf;

    /// Reads the workflow from the given path.
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories.
    async fn read(&self, path: Option<&Path>) -> anyhow::Result<Workflow>;

    /// Reads the workflow from the given path and merges it with an default
    /// workflow.
    async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let workflow = self.read(path).await?;
        let mut base_workflow = Workflow::default();
        base_workflow.merge(workflow);
        Ok(base_workflow)
    }

    /// Writes the given workflow to the specified path.
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories.
    async fn write(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()>;

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
    async fn collect(&self, max_depth: Option<usize>) -> anyhow::Result<Vec<File>>;
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
pub trait FsPatchService: Send + Sync {
    /// Patches a file at the specified path with the given content.
    async fn patch(
        &self,
        path: String,
        search: String,
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

/// Core app trait providing access to services and repositories.
/// This trait follows clean architecture principles for dependency management
/// and service/repository composition.
pub trait Services: Send + Sync + 'static + Clone {
    type ProviderService: ProviderService;
    type ConversationService: ConversationService;
    type TemplateService: TemplateService;
    type AttachmentService: AttachmentService;
    type EnvironmentService: EnvironmentService;
    type WorkflowService: WorkflowService + Sync;
    type FileDiscoveryService: FileDiscoveryService;
    type McpConfigManager: McpConfigManager;
    type FsCreateService: FsCreateService;
    type FsPatchService: FsPatchService;
    type FsReadService: FsReadService;
    type FsRemoveService: FsRemoveService;
    type FsSearchService: FsSearchService;
    type FollowUpService: FollowUpService;
    type FsUndoService: FsUndoService;
    type NetFetchService: NetFetchService;
    type ShellService: ShellService;
    type McpService: McpService;

    fn provider_service(&self) -> &Self::ProviderService;
    fn conversation_service(&self) -> &Self::ConversationService;
    fn template_service(&self) -> &Self::TemplateService;
    fn attachment_service(&self) -> &Self::AttachmentService;
    fn environment_service(&self) -> &Self::EnvironmentService;
    fn workflow_service(&self) -> &Self::WorkflowService;
    fn file_discovery_service(&self) -> &Self::FileDiscoveryService;
    fn mcp_config_manager(&self) -> &Self::McpConfigManager;
    fn fs_create_service(&self) -> &Self::FsCreateService;
    fn fs_patch_service(&self) -> &Self::FsPatchService;
    fn fs_read_service(&self) -> &Self::FsReadService;
    fn fs_remove_service(&self) -> &Self::FsRemoveService;
    fn fs_search_service(&self) -> &Self::FsSearchService;
    fn follow_up_service(&self) -> &Self::FollowUpService;
    fn fs_undo_service(&self) -> &Self::FsUndoService;
    fn net_fetch_service(&self) -> &Self::NetFetchService;
    fn shell_service(&self) -> &Self::ShellService;
    fn mcp_service(&self) -> &Self::McpService;
}
