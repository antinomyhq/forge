use std::path::Path;

use anyhow::Result;
use url::Url;

use crate::{
    AnyProvider, AppConfig, AuthCredential, Conversation, ConversationId, MigrationResult,
    Provider, ProviderId, Skill, Snapshot, SyncStatus, UserId, Workspace, WorkspaceAuth,
    WorkspaceId, WorkspaceSyncStatus,
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

    /// Permanently deletes a conversation
    ///
    /// # Arguments
    /// * `conversation_id` - The ID of the conversation to delete
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()>;
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
    async fn migrate_env_credentials(&self) -> anyhow::Result<Option<MigrationResult>>;
}

/// Repository for managing workspace metadata in local database
#[async_trait::async_trait]
pub trait WorkspaceRepository: Send + Sync {
    /// Save or update a workspace
    async fn upsert(
        &self,
        workspace_id: &WorkspaceId,
        user_id: &UserId,
        path: &std::path::Path,
    ) -> anyhow::Result<()>;

    /// Find workspace by path
    async fn find_by_path(&self, path: &std::path::Path) -> anyhow::Result<Option<Workspace>>;

    /// Get user ID from any workspace, or None if no workspaces exist
    async fn get_user_id(&self) -> anyhow::Result<Option<UserId>>;

    /// Delete workspace from local database
    async fn delete(&self, workspace_id: &WorkspaceId) -> anyhow::Result<()>;
}

/// Repository for managing workspace sync status and coordination
///
/// This repository provides operations for coordinating sync operations across
/// multiple processes, tracking sync status, and preventing concurrent syncs.
#[async_trait::async_trait]
pub trait WorkspaceSyncRepository: Send + Sync {
    /// Attempts to acquire the sync lock for a workspace
    ///
    /// Atomically checks if a sync is in progress and sets the status to
    /// IN_PROGRESS with the current process ID if not.
    ///
    /// # Arguments
    /// * `path` - Canonical path to the workspace
    /// * `process_id` - Process ID attempting to acquire the lock
    ///
    /// # Returns
    /// * `true` if lock was successfully acquired
    /// * `false` if another process holds the lock
    ///
    /// # Errors
    /// Returns an error if the database operation fails
    async fn try_acquire_lock(
        &self,
        path: &std::path::Path,
        process_id: u32,
    ) -> anyhow::Result<bool>;

    /// Releases the sync lock for a workspace
    ///
    /// Marks the sync as complete (SUCCESS status by default).
    /// Use update_status() to set FAILED status with error message.
    ///
    /// # Arguments
    /// * `path` - Canonical path to the workspace
    ///
    /// # Errors
    /// Returns an error if the database operation fails
    async fn release_lock(&self, path: &std::path::Path) -> anyhow::Result<()>;

    /// Updates the sync status for a workspace
    ///
    /// # Arguments
    /// * `path` - Canonical path to the workspace
    /// * `status` - New sync status
    /// * `error_message` - Optional error message if status is Failed
    ///
    /// # Errors
    /// Returns an error if the database operation fails
    async fn update_status(
        &self,
        path: &std::path::Path,
        status: SyncStatus,
        error_message: Option<String>,
    ) -> anyhow::Result<()>;

    /// Retrieves the current sync status for a workspace
    ///
    /// # Arguments
    /// * `path` - Canonical path to the workspace
    ///
    /// # Errors
    /// Returns an error if the database operation fails
    async fn get_status(
        &self,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<WorkspaceSyncStatus>>;

    /// Clears any stale IN_PROGRESS locks for a workspace
    ///
    /// This should be called on application startup to clear locks from
    /// crashed or interrupted processes. Resets IN_PROGRESS status to SUCCESS.
    ///
    /// # Arguments
    /// * `path` - Canonical path to the workspace
    ///
    /// # Errors
    /// Returns an error if the database operation fails
    /// Clears stale sync locks based on the configured sync interval
    ///
    /// Locks are considered stale if they've been IN_PROGRESS for more than 2x
    /// the sync interval The interval is read from the environment
    /// configuration
    async fn clear_stale_locks(&self, path: &std::path::Path) -> anyhow::Result<()>;
}

/// Repository for managing codebase indexing and search operations
#[async_trait::async_trait]
pub trait ContextEngineRepository: Send + Sync {
    /// Authenticate with the indexing service via gRPC API
    async fn authenticate(&self) -> anyhow::Result<WorkspaceAuth>;

    /// Create a new workspace on the indexing server
    async fn create_workspace(
        &self,
        working_dir: &std::path::Path,
        auth_token: &crate::ApiKey,
    ) -> anyhow::Result<WorkspaceId>;

    /// Upload files to be indexed
    async fn upload_files(
        &self,
        upload: &crate::FileUpload,
        auth_token: &crate::ApiKey,
    ) -> anyhow::Result<crate::FileUploadInfo>;

    /// Search the indexed codebase using semantic search
    async fn search(
        &self,
        query: &crate::CodeSearchQuery<'_>,
        auth_token: &crate::ApiKey,
    ) -> anyhow::Result<Vec<crate::Node>>;

    /// List all workspaces for a user
    async fn list_workspaces(
        &self,
        auth_token: &crate::ApiKey,
    ) -> anyhow::Result<Vec<crate::WorkspaceInfo>>;

    /// Get workspace information by workspace ID
    async fn get_workspace(
        &self,
        workspace_id: &WorkspaceId,
        auth_token: &crate::ApiKey,
    ) -> anyhow::Result<Option<crate::WorkspaceInfo>>;

    /// List all files in a workspace with their hashes
    async fn list_workspace_files(
        &self,
        workspace: &crate::WorkspaceFiles,
        auth_token: &crate::ApiKey,
    ) -> anyhow::Result<Vec<crate::FileHash>>;

    /// Delete files from a workspace
    async fn delete_files(
        &self,
        deletion: &crate::FileDeletion,
        auth_token: &crate::ApiKey,
    ) -> anyhow::Result<()>;

    /// Delete a workspace and all its indexed data
    async fn delete_workspace(
        &self,
        workspace_id: &WorkspaceId,
        auth_token: &crate::ApiKey,
    ) -> anyhow::Result<()>;
}

/// Repository for managing skills
///
/// This repository provides operations for loading and managing skills from
/// markdown files.
#[async_trait::async_trait]
pub trait SkillRepository: Send + Sync {
    /// Loads all available skills from the skills directory
    ///
    /// # Errors
    /// Returns an error if skill loading fails
    async fn load_skills(&self) -> Result<Vec<Skill>>;
}

/// Repository for validating file syntax
///
/// This repository provides operations for validating the syntax of source
/// code files using remote validation services.
#[async_trait::async_trait]
pub trait ValidationRepository: Send + Sync {
    /// Validates the syntax of a single file
    ///
    /// # Arguments
    /// * `path` - Path to the file (used for determining language and in error
    ///   messages)
    /// * `content` - Content of the file to validate
    ///
    /// # Returns
    /// * `Ok(None)` - File is valid or file type is not supported by backend
    /// * `Ok(Some(String))` - Validation failed with error message
    /// * `Err(_)` - Communication error with validation service
    async fn validate_file(
        &self,
        path: impl AsRef<std::path::Path> + Send,
        content: &str,
    ) -> Result<Option<String>>;
}
