use std::path::Path;

use anyhow::Result;
use url::Url;

use crate::{
    AnyProvider, AppConfig, AuthCredential, Conversation, ConversationId, IndexWorkspaceId,
    Provider, ProviderId, Snapshot, UserId,
};

/// Repository for managing file snapshots
///
/// This repository provides operations for creating and restoring file
/// snapshots, enabling undo functionality for file modifications.
#[async_trait::async_trait]
pub trait SnapshotRepository: Send + Sync {
    /// Inserts a new snapshot for the given file path
    ///
    /// # Arguments
    /// * `file_path` - Path to the file to snapshot
    ///
    /// # Errors
    /// Returns an error if the snapshot creation fails
    async fn insert_snapshot(&self, file_path: &Path) -> Result<Snapshot>;

    /// Restores the most recent snapshot for the given file path
    ///
    /// # Arguments
    /// * `file_path` - Path to the file to restore
    ///
    /// # Errors
    /// Returns an error if no snapshot exists or restoration fails
    async fn undo_snapshot(&self, file_path: &Path) -> Result<()>;
}

/// Repository for managing conversation persistence
///
/// This repository provides CRUD operations for conversations, including
/// creating, retrieving, and listing conversations.
#[async_trait::async_trait]
pub trait ConversationRepository: Send + Sync {
    /// Creates or updates a conversation
    ///
    /// # Arguments
    /// * `conversation` - The conversation to persist
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn upsert_conversation(&self, conversation: Conversation) -> Result<()>;

    /// Retrieves a conversation by its ID
    ///
    /// # Arguments
    /// * `conversation_id` - The ID of the conversation to retrieve
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn get_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<Conversation>>;

    /// Retrieves all conversations with an optional limit
    ///
    /// # Arguments
    /// * `limit` - Optional maximum number of conversations to retrieve
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn get_all_conversations(
        &self,
        limit: Option<usize>,
    ) -> Result<Option<Vec<Conversation>>>;

    /// Retrieves the most recent conversation
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn get_last_conversation(&self) -> Result<Option<Conversation>>;
}

#[async_trait::async_trait]
pub trait AppConfigRepository: Send + Sync {
    async fn get_app_config(&self) -> anyhow::Result<AppConfig>;
    async fn set_app_config(&self, config: &AppConfig) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait ProviderRepository: Send + Sync {
    async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>>;
    async fn get_provider(&self, id: ProviderId) -> anyhow::Result<Provider<Url>>;
    async fn upsert_credential(&self, credential: AuthCredential) -> anyhow::Result<()>;
    async fn get_credential(&self, id: &ProviderId) -> anyhow::Result<Option<AuthCredential>>;
    async fn remove_credential(&self, id: &ProviderId) -> anyhow::Result<()>;
}

/// Domain entity for indexed workspace
#[derive(Debug, Clone)]
pub struct Workspace {
    pub workspace_id: IndexWorkspaceId,
    pub user_id: UserId,
    pub path: std::path::PathBuf,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Repository for managing indexed workspaces
#[async_trait::async_trait]
pub trait WorkspaceRepository: Send + Sync {
    /// Save or update an indexed workspace
    async fn upsert(
        &self,
        workspace_id: &IndexWorkspaceId,
        user_id: &UserId,
        path: &std::path::Path,
    ) -> Result<()>;

    /// Find indexed workspace by path
    async fn find_by_path(&self, path: &std::path::Path) -> Result<Option<Workspace>>;

    /// Get user ID from any indexed workspace
    async fn get_user_id(&self) -> Result<Option<UserId>>;
}

/// Repository for managing codebase indexing and search operations
///
/// This repository provides operations for creating workspaces, uploading
/// files, searching indexed codebases, and managing workspace files.
#[async_trait::async_trait]
pub trait CodebaseRepository: Send + Sync {
    /// Create a new workspace on the indexing server
    ///
    /// # Arguments
    /// * `user_id` - The user ID owning the workspace
    /// * `working_dir` - The working directory path
    ///
    /// # Errors
    /// Returns an error if workspace creation fails
    async fn create_workspace(
        &self,
        user_id: &UserId,
        working_dir: &std::path::Path,
    ) -> anyhow::Result<IndexWorkspaceId>;

    /// Upload files to be indexed
    ///
    /// # Arguments
    /// * `user_id` - The user ID owning the workspace
    /// * `workspace_id` - The workspace ID to upload files to
    /// * `files` - Vector of files to upload
    ///
    /// # Errors
    /// Returns an error if file upload fails
    async fn upload_files(
        &self,
        user_id: &UserId,
        workspace_id: &IndexWorkspaceId,
        files: Vec<crate::FileRead>,
    ) -> anyhow::Result<crate::UploadStats>;

    /// Search the indexed codebase using semantic search
    ///
    /// # Arguments
    /// * `user_id` - The user ID owning the workspace
    /// * `workspace_id` - The workspace ID to search in
    /// * `query` - The search query string
    /// * `limit` - Maximum number of results to return
    /// * `top_k` - Optional top-k parameter for search ranking
    ///
    /// # Errors
    /// Returns an error if the search operation fails
    async fn search(
        &self,
        user_id: &UserId,
        workspace_id: &IndexWorkspaceId,
        query: &str,
        limit: usize,
        top_k: Option<u32>,
    ) -> anyhow::Result<Vec<crate::CodeSearchResult>>;

    /// List all workspaces for a user
    ///
    /// # Arguments
    /// * `user_id` - The user ID to list workspaces for
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn list_workspaces(&self, user_id: &UserId) -> anyhow::Result<Vec<crate::WorkspaceInfo>>;

    /// List all files in a workspace with their hashes
    ///
    /// # Arguments
    /// * `user_id` - The user ID owning the workspace
    /// * `workspace_id` - The workspace ID to list files from
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn list_workspace_files(
        &self,
        user_id: &UserId,
        workspace_id: &IndexWorkspaceId,
    ) -> anyhow::Result<Vec<crate::FileHash>>;

    /// Delete files from a workspace
    ///
    /// # Arguments
    /// * `user_id` - The user ID owning the workspace
    /// * `workspace_id` - The workspace ID to delete files from
    /// * `file_paths` - Vector of file paths to delete
    ///
    /// # Errors
    /// Returns an error if the deletion fails
    async fn delete_files(
        &self,
        user_id: &UserId,
        workspace_id: &IndexWorkspaceId,
        file_paths: Vec<String>,
    ) -> anyhow::Result<()>;
}
