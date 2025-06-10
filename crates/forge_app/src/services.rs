use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_domain::{
    Agent, Attachment, ChatCompletionMessage, CommandOutput, Context, Conversation, ConversationId,
    Environment, File, McpConfig, Model, ModelId, PatchOperation, ResultStream, Scope, Task,
    TaskId, Tool, ToolCallContext, ToolCallFull, ToolDefinition, ToolName, ToolResult, Workflow,
};

pub struct ShellOutput {
    pub output: CommandOutput,
    pub shell: String,
}

pub struct PatchOutput {
    pub warning: Option<String>,
    pub before: String,
    pub after: String,
}

pub struct ReadOutput {
    pub content: Content,
    pub start_line: u64,
    pub end_line: u64,
    pub total_lines: u64,
}

pub enum Content {
    File(String),
}

pub struct SearchResult {
    pub matches: Vec<String>,
}

pub struct FetchOutput {
    pub content: String,
    pub code: u16,
    pub context: String,
}

pub struct FsCreateOutput {
    pub path: String,
    // Set when the file already exists
    pub previous: Option<String>,
    pub warning: Option<String>,
}

pub struct FsRemoveOutput {
    pub completed: bool,
}

#[derive(derive_more::From)]
pub struct FsUndoOutput(String);

impl FsUndoOutput {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn into_inner(self) -> String {
        self.0
    }
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
pub trait ToolService: Send + Sync {
    // TODO: should take `call` by reference
    async fn call(
        &self,
        agent: &Agent,
        context: &mut ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult;
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>>;
    async fn find(&self, name: &ToolName) -> anyhow::Result<Option<Arc<Tool>>>;
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
    async fn find(&self, name: &ToolName) -> anyhow::Result<Option<Arc<Tool>>>;
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
    fn render(
        &self,
        template: impl ToString,
        object: &impl serde::Serialize,
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
pub trait TaskService: Send + Sync {
    /// Appends multiple tasks to the end of the task list atomically
    async fn append(&self, descriptions: Vec<String>) -> anyhow::Result<()>;

    /// Prepends multiple tasks to the beginning of the task list atomically
    async fn prepend(&self, descriptions: Vec<String>) -> anyhow::Result<()>;

    /// Marks the first pending task as in progress and returns it
    async fn pop_front(&self) -> anyhow::Result<Option<Task>>;

    /// Marks the last pending task as in progress and returns it
    async fn pop_back(&self) -> anyhow::Result<Option<Task>>;

    /// Marks a task as done by its ID
    async fn mark_done(&self, id: TaskId) -> anyhow::Result<Option<Task>>;

    /// Lists all tasks in the current task list
    async fn list(&self) -> anyhow::Result<Vec<Task>>;

    /// Gets statistics about the current task list
    async fn stats(&self) -> anyhow::Result<(u32, u32, u32, u32)>; // (total, done, pending, in_progress)

    /// Finds the next pending task
    async fn find_next_pending(&self) -> anyhow::Result<Option<Task>>;

    /// Formats tasks as markdown
    async fn format_markdown(&self) -> anyhow::Result<String>;
    /// Clears all tasks from the task list
    async fn clear(&self) -> anyhow::Result<()>;
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
    async fn fetch(&self, url: String, raw: Option<bool>) -> anyhow::Result<FetchOutput>;
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
    type ToolService: ToolService;
    type ProviderService: ProviderService;
    type ConversationService: ConversationService;
    type TemplateService: TemplateService;
    type AttachmentService: AttachmentService;
    type EnvironmentService: EnvironmentService;
    type WorkflowService: WorkflowService;
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
    type TaskService: TaskService;

    fn tool_service(&self) -> &Self::ToolService;
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
    fn task_service(&self) -> &Self::TaskService;
}
