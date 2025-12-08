use derive_more::Display;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{LineNumbers, WorkspaceId};

/// Progress events emitted during codebase indexing
#[derive(Debug, Clone, PartialEq)]
pub enum SyncProgress {
    /// Sync operation is starting
    Starting,
    /// A new workspace was created on the server
    WorkspaceCreated {
        /// The ID of the newly created workspace
        workspace_id: WorkspaceId,
    },
    /// Discovering files in the directory
    DiscoveringFiles {
        /// Path being scanned
        path: std::path::PathBuf,
    },
    /// Files have been discovered in the directory
    FilesDiscovered {
        /// Total number of files found
        count: usize,
    },
    /// Comparing local files with server state
    ComparingFiles {
        /// Number of remote files in the workspace
        remote_files: usize,
        /// Number of local files being compared
        local_files: usize,
    },
    /// Diff computed showing breakdown of changes
    DiffComputed {
        /// Number of files to delete (orphaned on server)
        to_delete: usize,
        /// Number of files to upload (new files)
        to_upload: usize,
        /// Number of modified files (delete + upload same path)
        modified: usize,
    },
    /// Syncing files (deleting outdated + uploading new/changed)
    Syncing {
        /// Current progress score (modified files contribute 0.5 for delete +
        /// 0.5 for upload)
        current: f64,
        /// Total number of files to sync
        total: usize,
    },
    /// Sync operation completed successfully
    Completed {
        /// Total number of files in the workspace
        total_files: usize,
        /// Number of files that were uploaded (changed or new)
        uploaded_files: usize,
    },
}

impl SyncProgress {
    /// Returns the progress weight (0-100) for this event.
    pub fn weight(&self) -> Option<u64> {
        match self {
            Self::Syncing { current, total } => {
                let sync_progress = if *total > 0 {
                    (*current * 100.0 / *total as f64) as u64
                } else {
                    0
                };
                Some(sync_progress)
            }
            _ => None,
        }
    }
}

/// Stored authentication token for the indexing service (no expiry)
///
/// Associates a user with their indexing service authentication token
/// obtained from the remote authentication API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceAuth {
    /// User ID that owns this authentication
    pub user_id: UserId,
    /// Authentication token (obtained from HTTP API)
    pub token: crate::ApiKey,
    /// When this token was stored locally
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<WorkspaceAuth> for crate::AuthDetails {
    fn from(auth: WorkspaceAuth) -> Self {
        crate::AuthDetails::ApiKey(auth.token)
    }
}

impl WorkspaceAuth {
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

#[derive(Debug, Clone, PartialEq, Eq, Setters)]
#[setters(strip_option, into)]
pub struct SearchParams<'a> {
    pub query: &'a str,
    pub limit: Option<usize>,
    pub top_k: Option<u32>,
    pub use_case: String,
    pub starts_with: Option<String>,
    pub ends_with: Option<String>,
}

impl<'a> SearchParams<'a> {
    pub fn new(query: &'a str, use_case: &str) -> Self {
        Self {
            query,
            limit: None,
            top_k: None,
            use_case: use_case.to_string(),
            starts_with: None,
            ends_with: None,
        }
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

/// Node identifier for code graph nodes.
///
/// Uniquely identifies a node in the codebase graph (file chunks, files,
/// notes, tasks, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[display("{}", _0)]
pub struct NodeId(String);

impl NodeId {
    /// Create a new node ID from a string
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the node ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for NodeId {
    fn as_ref(&self) -> &str {
        &self.0
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
    /// Number of nodes created
    pub node_count: u64,
    /// Number of relations between nodes
    pub relation_count: u64,
    /// Last updated timestamp
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
    /// Workspace created time.
    pub created_at: chrono::DateTime<chrono::Utc>,
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
pub struct FileUploadResponse {
    /// Workspace ID that was synced
    pub workspace_id: WorkspaceId,
    /// Number of files processed
    pub files_processed: usize,
    /// Upload statistics
    pub upload_stats: FileUploadInfo,
    /// Whether a new workspace was created (vs using existing)
    pub is_new_workspace: bool,
}

impl FileUploadResponse {
    /// Create new sync statistics
    pub fn new(
        workspace_id: WorkspaceId,
        files_processed: usize,
        upload_stats: FileUploadInfo,
    ) -> Self {
        Self {
            workspace_id,
            files_processed,
            upload_stats,
            is_new_workspace: false,
        }
    }
}

/// Statistics from uploading files to the codebase server
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FileUploadInfo {
    /// Number of code nodes created
    pub nodes_created: usize,
    /// Number of relations created
    pub relations_created: usize,
}

impl std::ops::Add for FileUploadInfo {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            nodes_created: self.nodes_created + other.nodes_created,
            relations_created: self.relations_created + other.relations_created,
        }
    }
}

impl FileUploadInfo {
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
    /// Relevance query used for re-ranking
    pub use_case: String,
    /// The search results for this query
    pub results: Vec<Node>,
}

impl CodebaseQueryResult {
    /// Convert to XML element for tool output
    pub fn to_element(&self) -> forge_template::Element {
        use forge_template::Element;

        let mut elem = Element::new("query_result")
            .attr("query", &self.query)
            .attr("use_case", &self.use_case)
            .attr("results", self.results.len());

        if self.results.is_empty() {
            elem = elem.text("No results found. Try using multiple queries with different phrasings, synonyms, or more specific use_case descriptions to improve search coverage.");
        } else {
            for result in &self.results {
                elem = elem.append(result.node.to_element());
            }
        }

        elem
    }
}

/// Results for multiple codebase search queries
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CodebaseSearchResults {
    /// Results for each query/use_case pair
    pub queries: Vec<CodebaseQueryResult>,
}

/// A search result with its similarity score
///
/// Wraps a code node with its semantic search scores,
/// keeping the scores separate from the node data itself.
#[derive(
    Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, derive_setters::Setters,
)]
#[setters(strip_option)]
pub struct Node {
    /// Node identifier
    pub node_id: NodeId,
    /// The node data (file, chunk, note, etc.)
    #[serde(flatten)]
    pub node: NodeData,
    /// Relevance score (most important ranking metric)
    pub relevance: Option<f32>,
    /// Distance score (second ranking metric, lower is better)
    pub distance: Option<f32>,
    /// Similarity score (third ranking metric, higher is better)
    pub similarity: Option<f32>,
}

/// Result of a semantic search query
///
/// Represents different types of nodes returned from the codebase service.
/// Each variant contains only the fields relevant to that node type.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeData {
    /// File chunk with precise line numbers
    FileChunk {
        /// File path
        file_path: String,
        /// Code content
        content: String,
        /// Start line in the file
        start_line: u32,
        /// End line in the file
        end_line: u32,
    },
    /// Full file content
    File {
        /// File path
        file_path: String,
        /// File content
        content: String,
        /// SHA-256 hash of the file content
        hash: String,
    },
    /// File reference (path only, no content)
    FileRef {
        /// File path
        file_path: String,
        /// SHA-256 hash of the file content
        file_hash: String,
    },
    /// Note content
    Note {
        /// Note content
        content: String,
    },
    /// Task description
    Task {
        /// Task description
        task: String,
    },
}

impl NodeData {
    pub fn to_element(&self) -> forge_template::Element {
        use forge_template::Element;

        match self {
            Self::FileChunk { file_path, content, start_line, end_line } => {
                let numbered_content = content.numbered_from(*start_line as usize);
                Element::new("file_chunk")
                    .attr("file_path", file_path)
                    .attr("lines", format!("{}-{}", start_line, end_line))
                    .cdata(numbered_content)
            }
            Self::File { file_path, content, .. } => {
                let numbered_content = content.numbered();
                Element::new("file")
                    .attr("file_path", file_path)
                    .cdata(numbered_content)
            }
            Self::FileRef { file_path, .. } => {
                Element::new("file_ref").attr("file_path", file_path)
            }
            Self::Note { content } => Element::new("note").cdata(content),
            Self::Task { task } => Element::new("task").text(task),
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

    #[test]
    fn test_search_params_with_file_extension() {
        let actual = SearchParams::new("retry mechanism", "find retry logic")
            .limit(10usize)
            .top_k(20u32)
            .ends_with(".rs");

        let expected = SearchParams {
            query: "retry mechanism",
            limit: Some(10),
            top_k: Some(20),
            use_case: "find retry logic".to_string(),
            starts_with: None,
            ends_with: Some(".rs".to_string()),
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_search_params_without_file_extension() {
        let actual = SearchParams::new("auth logic", "authentication implementation").limit(5usize);

        let expected = SearchParams {
            query: "auth logic",
            limit: Some(5),
            top_k: None,
            use_case: "authentication implementation".to_string(),
            starts_with: None,
            ends_with: None,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_codebase_query_result_empty_results() {
        let fixture = CodebaseQueryResult {
            query: "retry mechanism".to_string(),
            use_case: "find retry logic".to_string(),
            results: vec![],
        };

        let actual = fixture.to_element().render();
        insta::assert_snapshot!(actual);
    }

    #[test]
    fn test_codebase_query_result_with_results() {
        let fixture = CodebaseQueryResult {
            query: "auth logic".to_string(),
            use_case: "authentication".to_string(),
            results: vec![Node {
                node_id: "node-1".into(),
                node: NodeData::FileChunk {
                    file_path: "src/auth.rs".to_string(),
                    content: "fn authenticate() {}".to_string(),
                    start_line: 10,
                    end_line: 15,
                },
                relevance: Some(0.95),
                distance: Some(0.05),
                similarity: Some(0.95),
            }],
        };

        let actual = fixture.to_element().render();
        insta::assert_snapshot!(actual);
    }
}
