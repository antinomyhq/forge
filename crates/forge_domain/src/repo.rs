use std::path::Path;

use anyhow::Result;
use url::Url;

use crate::{
    AnyProvider, AppConfig, AuthCredential, Conversation, ConversationId, Provider, ProviderId,
    Snapshot, UserId, Workspace, WorkspaceId,
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

/// Repository for managing workspaces
///
/// This repository provides local database operations for workspace metadata,
/// tracking which workspaces have been synced with the codebase server.
#[async_trait::async_trait]
pub trait WorkspaceRepository: Send + Sync {
    /// Save or update a workspace
    ///
    /// # Arguments
    /// * `workspace_id` - The workspace ID from the codebase server
    /// * `user_id` - The user ID owning the workspace
    /// * `path` - The local filesystem path of the workspace
    ///
    /// # Errors
    /// Returns an error if the database operation fails
    async fn upsert(
        &self,
        workspace_id: &WorkspaceId,
        user_id: &UserId,
        path: &std::path::Path,
    ) -> anyhow::Result<()>;

    /// Find workspace by path
    ///
    /// # Arguments
    /// * `path` - The local filesystem path to search for
    ///
    /// # Errors
    /// Returns an error if the database query fails
    async fn find_by_path(&self, path: &std::path::Path) -> anyhow::Result<Option<Workspace>>;

    /// Get user ID from any workspace
    ///
    /// Returns the user ID from any existing workspace record,
    /// or None if no workspaces exist.
    ///
    /// # Errors
    /// Returns an error if the database query fails
    async fn get_user_id(&self) -> anyhow::Result<Option<UserId>>;
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
    ) -> anyhow::Result<WorkspaceId>;

    /// Upload files to be indexed
    ///
    /// # Arguments
    /// * `upload` - File upload parameters containing user_id, workspace_id,
    ///   and files
    ///
    /// # Errors
    /// Returns an error if file upload fails
    async fn upload_files(&self, upload: &crate::FileUpload) -> anyhow::Result<crate::UploadStats>;

    /// Search the indexed codebase using semantic search
    ///
    /// # Arguments
    /// * `query` - The search query parameters
    ///
    /// # Errors
    /// Returns an error if the search operation fails
    async fn search(
        &self,
        query: &crate::CodeSearchQuery<'_>,
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
    /// * `workspace` - Workspace parameters containing user_id and workspace_id
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn list_workspace_files(
        &self,
        workspace: &crate::WorkspaceFiles,
    ) -> anyhow::Result<Vec<crate::FileHash>>;

    /// Delete files from a workspace
    ///
    /// # Arguments
    /// * `deletion` - Deletion parameters containing user_id, workspace_id, and
    ///   file paths
    ///
    /// # Errors
    /// Returns an error if the deletion fails
    async fn delete_files(&self, deletion: &crate::FileDeletion) -> anyhow::Result<()>;
}
