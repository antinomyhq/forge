use derive_more::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User identifier for indexing operations.
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

/// Workspace identifier (UUID) from indexing server.
///
/// Generated locally and sent to server during CreateWorkspace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[display("{}", _0)]
pub struct IndexWorkspaceId(Uuid);

impl IndexWorkspaceId {
    /// Generate a new random workspace ID
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a workspace ID from a string
    ///
    /// # Errors
    /// Returns an error if the string is not a valid UUID
    pub fn from_string(s: &str) -> anyhow::Result<Self> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the inner UUID
    pub fn inner(&self) -> Uuid {
        self.0
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
    pub workspace_id: IndexWorkspaceId,
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

/// Result of an indexing operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct IndexStats {
    /// Workspace ID that was indexed
    pub workspace_id: IndexWorkspaceId,
    /// Number of files processed
    pub files_processed: usize,
    /// Upload statistics
    pub upload_stats: UploadStats,
}

impl IndexStats {
    /// Create new index statistics
    pub fn new(
        workspace_id: IndexWorkspaceId,
        files_processed: usize,
        upload_stats: UploadStats,
    ) -> Self {
        Self { workspace_id, files_processed, upload_stats }
    }
}

/// Statistics from uploading files to the indexing server
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UploadStats {
    /// Number of code nodes created
    pub nodes_created: usize,
    /// Number of relations created
    pub relations_created: usize,
}

impl Default for UploadStats {
    fn default() -> Self {
        Self::new(0, 0)
    }
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

/// Result of a semantic search query
///
/// Represents different types of nodes returned from the indexing service.
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
        let workspace_id = IndexWorkspaceId::generate();
        let s = workspace_id.to_string();
        let parsed = IndexWorkspaceId::from_string(&s).unwrap();
        assert_eq!(workspace_id, parsed);
    }
}
