use derive_more::Display;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::WorkspaceId;

/// Stored authentication token for the indexing service (no expiry)
///
/// Associates a user with their indexing service authentication token
/// obtained from the remote authentication API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexingAuth {
    /// User ID that owns this authentication
    pub user_id: UserId,
    /// Authentication token (obtained from HTTP API)
    pub token: crate::ApiKey,
    /// When this token was stored locally
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl IndexingAuth {
    /// Create a new indexing auth record
    pub fn new(user_id: UserId, token: crate::ApiKey) -> Self {
        Self { user_id, token, created_at: chrono::Utc::now() }
    }
}

/// File content for upload to codebase server
///
/// Contains the file path (relative to workspace root) and its textual content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRead {
    /// File path (relative to workspace root)
    pub path: String,
    /// File content as UTF-8 text
    pub content: String,
}

impl FileRead {
    /// Create a new file read entry
    pub fn new(path: String, content: String) -> Self {
        Self { path, content }
    }
}

/// Generic wrapper for codebase operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBase<T> {
    pub user_id: UserId,
    pub workspace_id: WorkspaceId,
    pub data: T,
}

impl<T> CodeBase<T> {
    pub fn new(user_id: UserId, workspace_id: WorkspaceId, data: T) -> Self {
        Self { user_id, workspace_id, data }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchParams<'a> {
    pub query: &'a str,
    pub limit: usize,
    pub top_k: Option<u32>,
}

impl<'a> SearchParams<'a> {
    pub fn new(query: &'a str, limit: usize) -> Self {
        Self { query, limit, top_k: None }
    }

    pub fn with_top_k(mut self, top_k: u32) -> Self {
        self.top_k = Some(top_k);
        self
    }
}

pub type CodeSearchQuery<'a> = CodeBase<SearchParams<'a>>;
pub type FileUpload = CodeBase<Vec<FileRead>>;
pub type FileDeletion = CodeBase<Vec<String>>;
pub type WorkspaceFiles = CodeBase<()>;

/// User identifier for codebase operations.
///
/// Unique per machine, generated once and stored in database.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[display("{}", _0)]
pub struct UserId(Uuid);

impl UserId {
    /// Generate a new random user ID
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a user ID from a string
    ///
    /// # Errors
    /// Returns an error if the string is not a valid UUID
    pub fn from_string(s: &str) -> anyhow::Result<Self> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Git repository information for a workspace
///
/// Contains commit hash and branch name for version tracking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitInfo {
    /// Git commit hash (e.g., "abc123...")
    pub commit: String,
    /// Git branch name (e.g., "main", "develop")
    pub branch: String,
}

/// Information about a workspace from the server
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    /// Workspace ID
    pub workspace_id: WorkspaceId,
    /// Working directory path
    pub working_dir: String,
}

/// File hash information from the server
///
/// Contains the relative file path and its SHA-256 hash
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHash {
    /// Relative file path from workspace root
    pub path: String,
    /// SHA-256 hash of the file content
    pub hash: String,
}

/// Result of a codebase sync operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
pub struct IndexStats {
    /// Workspace ID that was synced
    pub workspace_id: WorkspaceId,
    /// Number of files processed
    pub files_processed: usize,
    /// Upload statistics
    pub upload_stats: UploadStats,
    /// New API key that was created (if any)
    pub new_api_key: Option<IndexingAuth>,
    /// Whether a new workspace was created (vs using existing)
    pub is_new_workspace: bool,
}

impl IndexStats {
    /// Create new sync statistics
    pub fn new(
        workspace_id: WorkspaceId,
        files_processed: usize,
        upload_stats: UploadStats,
    ) -> Self {
        Self {
            workspace_id,
            files_processed,
            upload_stats,
            new_api_key: None,
            is_new_workspace: false,
        }
    }

    /// Set the new API key that was created during sync
    pub fn with_new_api_key(mut self, new_api_key: Option<IndexingAuth>) -> Self {
        self.new_api_key = new_api_key;
        self
    }

    /// Set whether a new workspace was created
    pub fn with_new_workspace(mut self, is_new: bool) -> Self {
        self.is_new_workspace = is_new;
        self
    }
}

/// Statistics from uploading files to the codebase server
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UploadStats {
    /// Number of code nodes created
    pub nodes_created: usize,
    /// Number of relations created
    pub relations_created: usize,
}

impl std::ops::Add for UploadStats {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            nodes_created: self.nodes_created + other.nodes_created,
            relations_created: self.relations_created + other.relations_created,
        }
    }
}

impl UploadStats {
    /// Create new upload statistics
    pub fn new(nodes_created: usize, relations_created: usize) -> Self {
        Self { nodes_created, relations_created }
    }
}

/// Results for a single codebase search query
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CodebaseQueryResult {
    /// The query string that was executed
    pub query: String,
    /// The search results for this query
    pub results: Vec<CodeSearchResult>,
}

/// Result of a semantic search query
///
/// Represents different types of nodes returned from the codebase service.
/// Each variant contains only the fields relevant to that node type.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodeSearchResult {
    /// File chunk with precise line numbers
    FileChunk {
        /// Node ID
        node_id: String,
        /// File path
        file_path: String,
        /// Code content
        content: String,
        /// Start line in the file
        start_line: u32,
        /// End line in the file
        end_line: u32,
        /// Similarity score (0.0 - 1.0)
        similarity: f32,
    },
    /// Full file content
    File {
        /// Node ID
        node_id: String,
        /// File path
        file_path: String,
        /// File content
        content: String,
        /// SHA-256 hash of the file content
        hash: String,
        /// Similarity score (0.0 - 1.0)
        similarity: f32,
    },
    /// File reference (path only, no content)
    FileRef {
        /// Node ID
        node_id: String,
        /// File path
        file_path: String,
        /// SHA-256 hash of the file content
        file_hash: String,
        /// Similarity score (0.0 - 1.0)
        similarity: f32,
    },
    /// Note content
    Note {
        /// Node ID
        node_id: String,
        /// Note content
        content: String,
        /// Similarity score (0.0 - 1.0)
        similarity: f32,
    },
    /// Task description
    Task {
        /// Node ID
        node_id: String,
        /// Task description
        task: String,
        /// Similarity score (0.0 - 1.0)
        similarity: f32,
    },
}

impl CodeSearchResult {
    /// Get the node ID for any variant
    pub fn node_id(&self) -> &str {
        match self {
            Self::FileChunk { node_id, .. }
            | Self::File { node_id, .. }
            | Self::FileRef { node_id, .. }
            | Self::Note { node_id, .. }
            | Self::Task { node_id, .. } => node_id,
        }
    }

    /// Get the similarity score for any variant
    pub fn similarity(&self) -> f32 {
        match self {
            Self::FileChunk { similarity, .. }
            | Self::File { similarity, .. }
            | Self::FileRef { similarity, .. }
            | Self::Note { similarity, .. }
            | Self::Task { similarity, .. } => *similarity,
        }
    }

    pub fn to_element(&self) -> forge_template::Element {
        use forge_template::Element;

        match self {
            Self::FileChunk { file_path, content, start_line, end_line, similarity, .. } => {
                Element::new("file_chunk")
                    .attr("file_path", file_path)
                    .attr("lines", format!("{}-{}", start_line, end_line))
                    .attr("similarity", format!("{:.2}", similarity))
                    .cdata(content)
            }
            Self::File { file_path, content, similarity, .. } => Element::new("file")
                .attr("file_path", file_path)
                .attr("similarity", format!("{:.2}", similarity))
                .cdata(content),
            Self::FileRef { file_path, similarity, .. } => Element::new("file_ref")
                .attr("file_path", file_path)
                .attr("similarity", format!("{:.2}", similarity)),
            Self::Note { content, similarity, .. } => Element::new("note")
                .attr("similarity", format!("{:.2}", similarity))
                .cdata(content),
            Self::Task { task, similarity, .. } => Element::new("task")
                .attr("similarity", format!("{:.2}", similarity))
                .text(task),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_user_id_roundtrip() {
        let user_id = UserId::generate();
        let s = user_id.to_string();
        let parsed = UserId::from_string(&s).unwrap();
        assert_eq!(user_id, parsed);
    }

    #[test]
    fn test_workspace_id_roundtrip() {
        let workspace_id = WorkspaceId::generate();
        let s = workspace_id.to_string();
        let parsed = WorkspaceId::from_string(&s).unwrap();
        assert_eq!(workspace_id, parsed);
    }
}
