use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 Request wrapper
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub params: T,
}

/// JSON-RPC 2.0 Response wrapper
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 Error object
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ============================================================================
// Event Types (mirrors forge_domain::Event hierarchy)
// ============================================================================

/// User prompt text wrapper
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct UserPrompt(String);

impl From<forge_domain::UserPrompt> for UserPrompt {
    fn from(p: forge_domain::UserPrompt) -> Self {
        Self(p.to_string())
    }
}

/// User command with parameters
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct UserCommand {
    pub name: String,
    pub template: String,
    pub parameters: Vec<String>,
}

impl From<forge_domain::UserCommand> for UserCommand {
    fn from(c: forge_domain::UserCommand) -> Self {
        Self {
            name: c.name,
            template: c.template.template,
            parameters: c.parameters,
        }
    }
}

/// Event value variants (Text or Command)
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventValue {
    Text(UserPrompt),
    Command(UserCommand),
}

impl From<forge_domain::EventValue> for EventValue {
    fn from(v: forge_domain::EventValue) -> Self {
        match v {
            forge_domain::EventValue::Text(p) => EventValue::Text(UserPrompt::from(p)),
            forge_domain::EventValue::Command(c) => EventValue::Command(UserCommand::from(c)),
        }
    }
}

/// File info for attachment content
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct FileInfo {
    pub start_line: u64,
    pub end_line: u64,
    pub total_lines: u64,
    pub content_hash: String,
}

/// Directory entry for directory listings
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct DirectoryEntry {
    pub path: String,
    pub is_dir: bool,
}

/// Image attachment content
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct Image {
    pub url: String,
    pub mime_type: String,
}

/// Attachment content variants
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentContent {
    Image(Image),
    FileContent { content: String, info: FileInfo },
    DirectoryListing { entries: Vec<DirectoryEntry> },
}

/// File/directory attachment
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct Attachment {
    pub content: AttachmentContent,
    pub path: String,
}

/// Chat event containing user input
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct Event {
    pub id: String,
    pub value: Option<EventValue>,
    pub timestamp: String,
    pub attachments: Vec<Attachment>,
    pub additional_context: Option<String>,
}

impl From<forge_domain::Event> for Event {
    fn from(e: forge_domain::Event) -> Self {
        Self {
            id: e.id,
            value: e.value.map(EventValue::from),
            timestamp: e.timestamp,
            attachments: e
                .attachments
                .into_iter()
                .map(|a| Attachment {
                    content: match a.content {
                        forge_domain::AttachmentContent::Image(img) => {
                            AttachmentContent::Image(Image {
                                url: img.url().to_string(),
                                mime_type: img.mime_type().to_string(),
                            })
                        }
                        forge_domain::AttachmentContent::FileContent { content, info } => {
                            AttachmentContent::FileContent {
                                content,
                                info: FileInfo {
                                    start_line: info.start_line,
                                    end_line: info.end_line,
                                    total_lines: info.total_lines,
                                    content_hash: info.content_hash,
                                },
                            }
                        }
                        forge_domain::AttachmentContent::DirectoryListing { entries } => {
                            AttachmentContent::DirectoryListing {
                                entries: entries
                                    .into_iter()
                                    .map(|e| DirectoryEntry { path: e.path, is_dir: e.is_dir })
                                    .collect(),
                            }
                        }
                    },
                    path: a.path,
                })
                .collect(),
            additional_context: e.additional_context,
        }
    }
}

impl TryFrom<Event> for forge_domain::Event {
    type Error = anyhow::Error;

    fn try_from(e: Event) -> Result<Self, Self::Error> {
        Ok(Self {
            id: e.id,
            value: e.value.map(|v| v.try_into()).transpose()?,
            timestamp: e.timestamp,
            attachments: e
                .attachments
                .into_iter()
                .map(|a| -> anyhow::Result<forge_domain::Attachment> {
                    Ok(forge_domain::Attachment {
                        content: match a.content {
                            AttachmentContent::Image(img) => {
                                forge_domain::AttachmentContent::Image(
                                    forge_domain::Image::new_base64(img.url, img.mime_type),
                                )
                            }
                            AttachmentContent::FileContent { content, info } => {
                                forge_domain::AttachmentContent::FileContent {
                                    content,
                                    info: forge_domain::FileInfo {
                                        start_line: info.start_line,
                                        end_line: info.end_line,
                                        total_lines: info.total_lines,
                                        content_hash: info.content_hash,
                                    },
                                }
                            }
                            AttachmentContent::DirectoryListing { entries } => {
                                forge_domain::AttachmentContent::DirectoryListing {
                                    entries: entries
                                        .into_iter()
                                        .map(|e| forge_domain::DirectoryEntry {
                                            path: e.path,
                                            is_dir: e.is_dir,
                                        })
                                        .collect(),
                                }
                            }
                        },
                        path: a.path,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            additional_context: e.additional_context,
        })
    }
}

impl TryFrom<EventValue> for forge_domain::EventValue {
    type Error = anyhow::Error;

    fn try_from(v: EventValue) -> Result<Self, Self::Error> {
        match v {
            EventValue::Text(p) => {
                // UserPrompt has From<String> impl via derive_more::From
                let prompt: forge_domain::UserPrompt = p.0.into();
                Ok(forge_domain::EventValue::Text(prompt))
            }
            EventValue::Command(c) => Ok(forge_domain::EventValue::Command(
                forge_domain::UserCommand {
                    name: c.name,
                    template: c.template.into(),
                    parameters: c.parameters,
                },
            )),
        }
    }
}

// ============================================================================
// Request Parameters
// ============================================================================

/// Simple chat input for JSON-RPC
/// This is a simplified version of Event for easier API usage
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ChatInput {
    /// Message content as plain text
    pub message: String,
}

/// Request parameters for chat method (using simple input)
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ChatParams {
    pub conversation_id: String,
    pub message: String,
}

/// Request parameters for conversation operations
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationParams {
    pub conversation_id: String,
}

/// Request parameters for rename conversation
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RenameConversationParams {
    pub conversation_id: String,
    pub title: String,
}

/// Request parameters for shell command execution
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ShellCommandParams {
    pub command: String,
    #[serde(default)]
    pub working_dir: Option<String>,
}

/// Request parameters for workspace operations
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkspacePathParams {
    pub path: String,
}

/// Request parameters for workspace sync
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SyncWorkspaceParams {
    pub path: String,
}

/// Request parameters for workspace query
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QueryWorkspaceParams {
    pub path: String,
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Model configuration for config operations
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ModelConfigDto {
    pub provider_id: String,
    pub model_id: String,
}

/// Individual config operation types
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ConfigOperationDto {
    /// Set the active session provider and model
    SetSessionConfig { config: ModelConfigDto },
    /// Set the commit-message generation configuration
    SetCommitConfig { config: Option<ModelConfigDto> },
    /// Set the shell-command suggestion configuration
    SetSuggestConfig { config: ModelConfigDto },
    /// Set the reasoning effort level
    SetReasoningEffort { effort: String },
}

impl ConfigOperationDto {
    /// Convert DTO to domain ConfigOperation
    pub fn into_domain(self) -> anyhow::Result<forge_domain::ConfigOperation> {
        use std::str::FromStr;

        match self {
            ConfigOperationDto::SetSessionConfig { config } => {
                let provider_id = forge_domain::ProviderId::from_str(&config.provider_id)
                    .map_err(|e| anyhow::anyhow!("Invalid provider_id: {}", e))?;
                let model_id = forge_domain::ModelId::new(&config.model_id);
                Ok(forge_domain::ConfigOperation::SetSessionConfig(
                    forge_domain::ModelConfig::new(provider_id, model_id),
                ))
            }
            ConfigOperationDto::SetCommitConfig { config } => {
                let model_config = match config {
                    Some(c) => {
                        let provider_id = forge_domain::ProviderId::from_str(&c.provider_id)
                            .map_err(|e| anyhow::anyhow!("Invalid provider_id: {}", e))?;
                        let model_id = forge_domain::ModelId::new(&c.model_id);
                        Some(forge_domain::ModelConfig::new(provider_id, model_id))
                    }
                    None => None,
                };
                Ok(forge_domain::ConfigOperation::SetCommitConfig(model_config))
            }
            ConfigOperationDto::SetSuggestConfig { config } => {
                let provider_id = forge_domain::ProviderId::from_str(&config.provider_id)
                    .map_err(|e| anyhow::anyhow!("Invalid provider_id: {}", e))?;
                let model_id = forge_domain::ModelId::new(&config.model_id);
                Ok(forge_domain::ConfigOperation::SetSuggestConfig(
                    forge_domain::ModelConfig::new(provider_id, model_id),
                ))
            }
            ConfigOperationDto::SetReasoningEffort { effort } => {
                let effort = match effort.as_str() {
                    "low" => forge_domain::Effort::Low,
                    "medium" => forge_domain::Effort::Medium,
                    "high" => forge_domain::Effort::High,
                    _ => return Err(anyhow::anyhow!("Invalid effort level: {}", effort)),
                };
                Ok(forge_domain::ConfigOperation::SetReasoningEffort(effort))
            }
        }
    }
}

/// Request parameters for config operations using typed DTOs
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConfigParams {
    /// List of config operations to apply
    pub ops: Vec<ConfigOperationDto>,
}

/// Request parameters for MCP config operations
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct McpConfigParams {
    #[serde(default)]
    pub scope: Option<String>,
}

/// Request parameters for provider auth
/// Request parameters for initializing provider auth
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ProviderAuthParams {
    pub provider_id: String,
    pub method: String,
}

/// Request parameters for completing provider auth
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CompleteProviderAuthParams {
    pub provider_id: String,
    /// The auth flow type that was initiated
    pub flow_type: String,
    /// For API key flow: the API key value
    #[serde(default)]
    pub api_key: Option<String>,
    /// For API key flow: URL parameters to include
    #[serde(default)]
    pub url_params: Option<std::collections::HashMap<String, String>>,
    /// For Authorization code flow: the authorization code
    #[serde(default)]
    pub code: Option<String>,
    /// Timeout in seconds (default: 60)
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// Request parameters for setting active agent
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SetActiveAgentParams {
    pub agent_id: String,
}

/// Request parameters for commit
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommitParams {
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub max_diff_size: Option<usize>,
    #[serde(default)]
    pub diff: Option<String>,
    #[serde(default)]
    pub additional_context: Option<String>,
}

/// Request parameters for compact conversation
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CompactConversationParams {
    pub conversation_id: String,
}

/// Request parameters for generate command
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GenerateCommandParams {
    pub prompt: String,
}

/// Request parameters for generate data
/// Matches forge_domain::DataGenerationParameters
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GenerateDataParams {
    /// Path to input JSONL file for data generation
    pub input: String,
    /// Path to JSON schema file for LLM tool definition
    pub schema: String,
    /// Path to Handlebars template file for system prompt (optional)
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Path to Handlebars template file for user prompt (optional)
    #[serde(default)]
    pub user_prompt: Option<String>,
    /// Maximum number of concurrent LLM requests (default: 1)
    #[serde(default)]
    pub concurrency: Option<usize>,
}

/// Request parameters for delete workspaces
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DeleteWorkspacesParams {
    pub workspace_ids: Vec<String>,
}

/// Request parameters for MCP auth
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct McpAuthParams {
    pub server_url: String,
}

/// Request parameters for MCP logout
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct McpLogoutParams {
    #[serde(default)]
    pub server_url: Option<String>,
}

/// Request parameters for MCP auth status
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct McpAuthStatusParams {
    pub server_url: String,
}

/// Request parameters for writing MCP config
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WriteMcpConfigParams {
    pub scope: String,
    pub config: serde_json::Value,
}

/// Request parameters for initializing workspace
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct InitWorkspaceParams {
    pub path: String,
}

// ============================================================================
// Response DTOs (mirrors forge_domain types)
// ============================================================================

/// Model response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ModelResponse {
    pub id: String,
    pub name: Option<String>,
    pub provider: String,
}

impl From<forge_domain::Model> for ModelResponse {
    fn from(m: forge_domain::Model) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name,
            provider: "unknown".to_string(), // Model doesn't have provider in domain
        }
    }
}

/// Agent response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

impl From<forge_domain::Agent> for AgentResponse {
    fn from(a: forge_domain::Agent) -> Self {
        Self {
            id: a.id.to_string(),
            name: a.title.unwrap_or_else(|| a.id.to_string()),
            description: None, // Agent doesn't have description field in domain
        }
    }
}

/// File discovery response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FileResponse {
    pub path: String,
    pub is_dir: bool,
}

impl From<forge_domain::File> for FileResponse {
    fn from(f: forge_domain::File) -> Self {
        Self { path: f.path, is_dir: f.is_dir }
    }
}

/// Conversation response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConversationResponse {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<usize>,
}

impl From<forge_domain::Conversation> for ConversationResponse {
    fn from(c: forge_domain::Conversation) -> Self {
        Self {
            id: c.id.to_string(),
            title: c.title,
            created_at: c.metadata.created_at.to_string(),
            updated_at: c.metadata.updated_at.as_ref().map(|t| t.to_string()),
            message_count: c.context.as_ref().map(|ctx| ctx.messages.len()),
        }
    }
}

/// Command output response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CommandOutputResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

impl From<forge_domain::CommandOutput> for CommandOutputResponse {
    fn from(c: forge_domain::CommandOutput) -> Self {
        Self { stdout: c.stdout, stderr: c.stderr, exit_code: c.exit_code }
    }
}

/// Workspace info response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WorkspaceInfoResponse {
    pub workspace_id: String,
    pub working_dir: String,
    pub node_count: Option<u64>,
    pub relation_count: Option<u64>,
    pub last_updated: Option<String>,
    pub created_at: String,
}

impl From<forge_domain::WorkspaceInfo> for WorkspaceInfoResponse {
    fn from(w: forge_domain::WorkspaceInfo) -> Self {
        Self {
            workspace_id: w.workspace_id.to_string(),
            working_dir: w.working_dir,
            node_count: w.node_count,
            relation_count: w.relation_count,
            last_updated: w.last_updated.as_ref().map(|t| t.to_string()),
            created_at: w.created_at.to_string(),
        }
    }
}

/// File status response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FileStatusResponse {
    pub path: String,
    pub status: String,
}

impl From<forge_domain::FileStatus> for FileStatusResponse {
    fn from(f: forge_domain::FileStatus) -> Self {
        Self { path: f.path, status: format!("{:?}", f.status) }
    }
}

/// Compaction result response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CompactionResultResponse {
    pub original_tokens: usize,
    pub compacted_tokens: usize,
    pub original_messages: usize,
    pub compacted_messages: usize,
}

impl From<forge_domain::CompactionResult> for CompactionResultResponse {
    fn from(c: forge_domain::CompactionResult) -> Self {
        Self {
            original_tokens: c.original_tokens,
            compacted_tokens: c.compacted_tokens,
            original_messages: c.original_messages,
            compacted_messages: c.compacted_messages,
        }
    }
}

/// Stream message for subscription-based streaming
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamMessage {
    Chunk { data: serde_json::Value },
    Error { message: String },
    Complete,
}

/// Provider response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ProviderResponse {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

impl From<forge_domain::AnyProvider> for ProviderResponse {
    fn from(p: forge_domain::AnyProvider) -> Self {
        let id = p.id();
        let name = p
            .response()
            .map(|r| format!("{:?}", r))
            .unwrap_or_else(|| id.to_string());
        let api_key = match &p {
            forge_domain::AnyProvider::Url(provider) => provider
                .credential
                .as_ref()
                .map(|_| "configured".to_string()),
            _ => None,
        };
        Self { id: id.to_string(), name, api_key }
    }
}

/// User info response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct UserInfoResponse {
    pub auth_provider_id: String,
}

impl From<forge_app::User> for UserInfoResponse {
    fn from(u: forge_app::User) -> Self {
        Self { auth_provider_id: u.auth_provider_id.into_string() }
    }
}

/// User usage response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct UserUsageResponse {
    pub plan_type: String,
    pub current: u32,
    pub limit: u32,
    pub remaining: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_in: Option<u64>,
}

impl From<forge_app::UserUsage> for UserUsageResponse {
    fn from(u: forge_app::UserUsage) -> Self {
        Self {
            plan_type: u.plan.r#type,
            current: u.usage.current,
            limit: u.usage.limit,
            remaining: u.usage.remaining,
            reset_in: u.usage.reset_in,
        }
    }
}

/// Command response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CommandResponse {
    pub name: String,
    pub description: String,
}

impl From<forge_domain::Command> for CommandResponse {
    fn from(c: forge_domain::Command) -> Self {
        Self { name: c.name, description: c.description }
    }
}

/// Skill response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SkillResponse {
    pub name: String,
    pub path: Option<String>,
    pub command: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<Vec<String>>,
}

impl From<forge_domain::Skill> for SkillResponse {
    fn from(s: forge_domain::Skill) -> Self {
        Self {
            name: s.name,
            path: s.path.map(|p| p.to_string_lossy().to_string()),
            command: s.command,
            description: s.description,
            resources: Some(
                s.resources
                    .into_iter()
                    .map(|r| r.to_string_lossy().to_string())
                    .collect(),
            )
            .filter(|r: &Vec<String>| !r.is_empty()),
        }
    }
}

/// Commit result response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CommitResultResponse {
    pub message: String,
    pub has_staged_files: bool,
}

impl From<forge_app::CommitResult> for CommitResultResponse {
    fn from(c: forge_app::CommitResult) -> Self {
        Self { message: c.message, has_staged_files: c.has_staged_files }
    }
}

/// Auth context request response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AuthContextRequestResponse {
    pub url: Option<String>,
    pub message: Option<String>,
}

// Note: AuthContextRequest is an enum (ApiKey, DeviceCode, Code)
// Implementing From would require pattern matching each variant
// For now, manual conversion is needed based on the specific variant

/// Node response DTO for workspace query
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct NodeResponse {
    pub node_id: String,
    pub path: Option<String>,
    pub content: Option<String>,
    pub relevance: Option<f32>,
    pub distance: Option<f32>,
}

impl From<forge_domain::Node> for NodeResponse {
    fn from(n: forge_domain::Node) -> Self {
        Self {
            node_id: n.node_id.to_string(),
            path: None, // NodeData doesn't directly expose path, would need pattern matching
            content: None, // NodeData content would need pattern matching
            relevance: n.relevance,
            distance: n.distance,
        }
    }
}

/// Sync progress response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SyncProgressResponse {
    pub processed_files: usize,
    pub total_files: usize,
    pub current_file: Option<String>,
}

/// Tools overview response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ToolsOverviewResponse {
    pub enabled: Vec<String>,
    pub disabled: Vec<String>,
}

impl From<forge_app::dto::ToolsOverview> for ToolsOverviewResponse {
    fn from(t: forge_app::dto::ToolsOverview) -> Self {
        Self {
            enabled: t.system.into_iter().map(|s| s.name.to_string()).collect(),
            disabled: vec![], // Not directly available in domain type
        }
    }
}

/// Provider models response DTO
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ProviderModelsResponse {
    pub provider_id: String,
    pub provider_name: String,
    pub models: Vec<ModelResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl From<forge_domain::ProviderModels> for ProviderModelsResponse {
    fn from(p: forge_domain::ProviderModels) -> Self {
        Self {
            provider_id: p.provider_id.to_string(),
            provider_name: p.provider_id.to_string(), // Use provider_id as name
            models: p.models.into_iter().map(ModelResponse::from).collect(),
            error: None,
        }
    }
}
