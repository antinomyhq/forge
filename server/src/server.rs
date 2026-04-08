use std::sync::Arc;

use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};
use tracing::{info, warn};

use crate::auth::authenticate;
use crate::chunker::chunk_file;
use crate::db::Database;
use crate::embedder::Embedder;
use crate::proto::forge_service_server::ForgeService;
use crate::proto::*;
use crate::db::WorkspaceRow;
use crate::qdrant::{ChunkPoint, QdrantStore};

/// Core gRPC service implementation for the Forge Workspace Server.
///
/// Handles all RPC methods defined in `forge.proto`, backed by
/// SQLite (metadata), Qdrant (vectors), and Ollama (embeddings).
pub struct ForgeServiceImpl {
    db: Arc<Database>,
    qdrant: Arc<QdrantStore>,
    embedder: Arc<Embedder>,
    chunk_min_size: u32,
    chunk_max_size: u32,
    /// Tracks the last time an API key was created for rate limiting.
    last_key_created: std::sync::Mutex<std::time::Instant>,
}

impl ForgeServiceImpl {
    /// Creates a new gRPC service instance.
    ///
    /// # Arguments
    /// * `db` - SQLite database for metadata
    /// * `qdrant` - Qdrant vector store
    /// * `embedder` - Ollama embedding client
    /// * `chunk_min_size` - Default minimum chunk size in bytes
    /// * `chunk_max_size` - Default maximum chunk size in bytes
    pub fn new(
        db: Arc<Database>,
        qdrant: Arc<QdrantStore>,
        embedder: Arc<Embedder>,
        chunk_min_size: u32,
        chunk_max_size: u32,
    ) -> Self {
        Self {
            db,
            qdrant,
            embedder,
            chunk_min_size,
            chunk_max_size,
            last_key_created: std::sync::Mutex::new(
                std::time::Instant::now() - std::time::Duration::from_secs(10),
            ),
        }
    }
}

/// Computes SHA-256 hex hash of file content.
///
/// MUST match the Forge client's `compute_hash` (`crates/forge_app/src/utils.rs:103-108`):
/// `sha2::Sha256` over `content.as_bytes()`, result as lowercase hex.
fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Parses a unix-seconds timestamp string into `prost_types::Timestamp`.
fn parse_timestamp(s: &str) -> Option<prost_types::Timestamp> {
    s.parse::<i64>()
        .ok()
        .map(|secs| prost_types::Timestamp { seconds: secs, nanos: 0 })
}

/// Converts a `WorkspaceRow` from SQLite into the proto `Workspace` message.
fn workspace_row_to_proto(row: WorkspaceRow) -> Workspace {
    Workspace {
        workspace_id: Some(WorkspaceId { id: row.workspace_id }),
        working_dir: row.working_dir,
        node_count: Some(row.node_count),
        relation_count: Some(0),
        last_updated: None,
        min_chunk_size: row.min_chunk_size,
        max_chunk_size: row.max_chunk_size,
        created_at: parse_timestamp(&row.created_at),
    }
}

/// Extracts `workspace_id` string from an optional proto `WorkspaceId`.
fn extract_workspace_id(ws_id: Option<WorkspaceId>) -> Result<String, Status> {
    ws_id
        .ok_or_else(|| Status::invalid_argument("Missing workspace_id"))
        .map(|w| w.id)
}

/// Authenticates the request and verifies the user owns the workspace.
///
/// Returns the `user_id` on success.
async fn authenticate_and_verify_owner<T>(
    db: &Database,
    request: &Request<T>,
    workspace_id: &str,
) -> Result<String, Status> {
    let user_id = authenticate(db, request).await?;
    let owns = db
        .verify_workspace_owner(workspace_id, &user_id)
        .await
        .map_err(|e| Status::internal(format!("Ownership check failed: {e}")))?;
    if !owns {
        return Err(Status::permission_denied("Workspace does not belong to this user"));
    }
    Ok(user_id)
}

/// Extension trait to convert `anyhow::Result` into `tonic::Status`.
trait IntoStatus<T> {
    /// Maps the error into `Status::internal` with the given context message.
    fn into_status(self, msg: &str) -> Result<T, Status>;
}

impl<T> IntoStatus<T> for anyhow::Result<T> {
    fn into_status(self, msg: &str) -> Result<T, Status> {
        self.map_err(|e| Status::internal(format!("{msg}: {e}")))
    }
}

#[tonic::async_trait]
impl ForgeService for ForgeServiceImpl {
    /// Creates a new API key (bootstrap method — no auth required).
    async fn create_api_key(
        &self,
        request: Request<CreateApiKeyRequest>,
    ) -> Result<Response<CreateApiKeyResponse>, Status> {
        // Rate limit: max 1 key per second
        {
            let mut last = self
                .last_key_created
                .lock()
                .map_err(|_| Status::internal("Rate limit lock poisoned"))?;
            if last.elapsed() < std::time::Duration::from_secs(1) {
                return Err(Status::resource_exhausted(
                    "Rate limited: wait before creating another API key",
                ));
            }
            *last = std::time::Instant::now();
        }

        let req = request.into_inner();
        let user_id_input = req.user_id.as_ref().map(|u| u.id.as_str());

        let (user_id, key) = self
            .db
            .create_api_key(user_id_input)
            .await
            .into_status("Failed to create API key")?;

        info!(user_id = %user_id, "API key created");

        Ok(Response::new(CreateApiKeyResponse {
            user_id: Some(UserId { id: user_id }),
            key,
        }))
    }

    /// Creates a workspace or returns the existing one for the same working_dir.
    async fn create_workspace(
        &self,
        request: Request<CreateWorkspaceRequest>,
    ) -> Result<Response<CreateWorkspaceResponse>, Status> {
        let user_id = authenticate(&self.db, &request).await?;
        let req = request.into_inner();

        let ws_def = req
            .workspace
            .ok_or_else(|| Status::invalid_argument("Missing workspace definition"))?;

        let min_chunk = if ws_def.min_chunk_size > 0 {
            ws_def.min_chunk_size
        } else {
            self.chunk_min_size
        };
        let max_chunk = if ws_def.max_chunk_size > 0 {
            ws_def.max_chunk_size
        } else {
            self.chunk_max_size
        };

        let (workspace_id, working_dir, created_at, is_new) = self
            .db
            .create_workspace(&user_id, &ws_def.working_dir, min_chunk, max_chunk)
            .await
            .into_status("Failed to create workspace")?;

        // Create Qdrant collection for new workspaces
        if is_new {
            self.qdrant
                .ensure_collection(&workspace_id)
                .await
                .into_status("Failed to create Qdrant collection")?;
        }

        info!(workspace_id = %workspace_id, working_dir = %working_dir, is_new = is_new, "Workspace ready");

        Ok(Response::new(CreateWorkspaceResponse {
            workspace: Some(Workspace {
                workspace_id: Some(WorkspaceId { id: workspace_id }),
                working_dir,
                node_count: Some(0),
                relation_count: Some(0),
                last_updated: None,
                min_chunk_size: min_chunk,
                max_chunk_size: max_chunk,
                created_at: parse_timestamp(&created_at),
            }),
        }))
    }

    /// Uploads files: chunks -> embeds -> upserts into Qdrant.
    ///
    /// For each file:
    /// 1. Compute SHA-256 hash of the full content (for ListFiles compatibility)
    /// 2. Delete existing chunks in Qdrant for this file path (handles re-uploads)
    /// 3. Split content into line-aware chunks
    /// 4. Batch-embed all chunks via Ollama
    /// 5. Upsert vectors + payloads into Qdrant
    /// 6. Update file_refs in SQLite
    async fn upload_files(
        &self,
        request: Request<UploadFilesRequest>,
    ) -> Result<Response<UploadFilesResponse>, Status> {
        let req_ref = request.get_ref();
        let workspace_id = extract_workspace_id(req_ref.workspace_id.clone())?;
        authenticate_and_verify_owner(&self.db, &request, &workspace_id).await?;
        let req = request.into_inner();

        let content = req
            .content
            .ok_or_else(|| Status::invalid_argument("Missing content"))?;

        let mut all_node_ids: Vec<String> = Vec::new();

        for file in content.files {
            // Step 1: Hash the FULL file content (before chunking)
            let file_hash = compute_hash(&file.content);

            // Step 2: Delete old chunks for this file path
            if let Err(e) = self
                .qdrant
                .delete_by_file_paths(&workspace_id, &[file.path.clone()])
                .await
            {
                warn!(file = %file.path, error = ?e, "Failed to delete old chunks, continuing");
            }

            // Step 3: Chunk the file
            let chunks = chunk_file(
                &file.path,
                &file.content,
                self.chunk_min_size,
                self.chunk_max_size,
            );

            if chunks.is_empty() {
                // Still register the file ref for empty files
                let node_id = uuid::Uuid::new_v4().to_string();
                self.db
                    .upsert_file_ref(&workspace_id, &file.path, &file_hash, &node_id)
                    .await
                    .into_status("Failed to store file ref")?;
                all_node_ids.push(node_id);
                continue;
            }

            // Step 4: Batch-embed all chunks
            let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
            let embeddings = self
                .embedder
                .embed_batch(&texts)
                .await
                .map_err(|e| Status::internal(format!("Embedding failed for {}: {e}", file.path)))?;

            // Step 5: Build Qdrant points and upsert
            let chunk_points: Vec<ChunkPoint> = chunks
                .into_iter()
                .zip(embeddings)
                .map(|(chunk, vector)| ChunkPoint {
                    id: uuid::Uuid::new_v4().to_string(),
                    vector,
                    file_path: chunk.path,
                    content: chunk.content,
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                })
                .collect();

            let node_ids = self
                .qdrant
                .upsert_chunks(&workspace_id, chunk_points)
                .await
                .into_status("Qdrant upsert failed")?;

            // Step 6: Store file ref in SQLite with the first node_id
            let primary_node_id = node_ids.first().cloned().unwrap_or_default();
            self.db
                .upsert_file_ref(&workspace_id, &file.path, &file_hash, &primary_node_id)
                .await
                .into_status("Failed to store file ref")?;

            all_node_ids.extend(node_ids);
        }

        info!(
            workspace_id = %workspace_id,
            nodes = all_node_ids.len(),
            "Files uploaded"
        );

        Ok(Response::new(UploadFilesResponse {
            result: Some(UploadResult {
                node_ids: all_node_ids,
                relations: vec![],
            }),
        }))
    }

    /// Lists all files in a workspace with their content hashes.
    async fn list_files(
        &self,
        request: Request<ListFilesRequest>,
    ) -> Result<Response<ListFilesResponse>, Status> {
        let req_ref = request.get_ref();
        let workspace_id = extract_workspace_id(req_ref.workspace_id.clone())?;
        authenticate_and_verify_owner(&self.db, &request, &workspace_id).await?;

        let refs = self
            .db
            .list_file_refs(&workspace_id)
            .await
            .into_status("Failed to list files")?;

        let files: Vec<FileRefNode> = refs
            .into_iter()
            .map(|(node_id, file_path, file_hash)| FileRefNode {
                node_id: Some(NodeId { id: node_id }),
                hash: file_hash.clone(),
                git: None,
                data: Some(FileRef {
                    path: file_path,
                    file_hash,
                }),
            })
            .collect();

        Ok(Response::new(ListFilesResponse { files }))
    }

    /// Deletes files from a workspace (both Qdrant vectors and SQLite refs).
    async fn delete_files(
        &self,
        request: Request<DeleteFilesRequest>,
    ) -> Result<Response<DeleteFilesResponse>, Status> {
        let req_ref = request.get_ref();
        let workspace_id = extract_workspace_id(req_ref.workspace_id.clone())?;
        authenticate_and_verify_owner(&self.db, &request, &workspace_id).await?;
        let req = request.into_inner();

        // Delete from Qdrant
        let deleted_nodes = self
            .qdrant
            .delete_by_file_paths(&workspace_id, &req.file_paths)
            .await
            .into_status("Failed to delete from Qdrant")?;

        // Delete from SQLite
        self.db
            .delete_file_refs(&workspace_id, &req.file_paths)
            .await
            .into_status("Failed to delete file refs")?;

        info!(workspace_id = %workspace_id, deleted = deleted_nodes, "Files deleted");

        Ok(Response::new(DeleteFilesResponse {
            deleted_nodes,
            deleted_relations: 0,
        }))
    }

    /// Semantic search: embed query -> ANN search in Qdrant -> return FileChunk nodes.
    async fn search(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<SearchResponse>, Status> {
        let req_ref = request.get_ref();
        let workspace_id = extract_workspace_id(req_ref.workspace_id.clone())?;
        authenticate_and_verify_owner(&self.db, &request, &workspace_id).await?;
        let req = request.into_inner();

        let query = req.query.unwrap_or_default();

        let prompt = query
            .prompt
            .ok_or_else(|| Status::invalid_argument("Missing search prompt"))?;

        let top_k = query.top_k.unwrap_or(10);
        let limit = query.limit.unwrap_or(top_k);

        // Embed the query
        let vector = self
            .embedder
            .embed_single(&prompt)
            .await
            .into_status("Failed to embed search query")?;

        // Search Qdrant
        let hits = self
            .qdrant
            .search(
                &workspace_id,
                vector,
                limit.max(top_k),
                &query.starts_with,
                &query.ends_with,
            )
            .await
            .into_status("Qdrant search failed")?;

        // Map results to proto QueryItems
        let items: Vec<QueryItem> = hits
            .into_iter()
            .enumerate()
            .map(|(i, hit)| QueryItem {
                node: Some(Node {
                    node_id: Some(NodeId { id: hit.id }),
                    workspace_id: Some(WorkspaceId { id: workspace_id.clone() }),
                    hash: String::new(),
                    git: None,
                    data: Some(NodeData {
                        kind: Some(node_data::Kind::FileChunk(FileChunk {
                            path: hit.file_path,
                            content: hit.content,
                            start_line: hit.start_line,
                            end_line: hit.end_line,
                        })),
                    }),
                }),
                distance: Some(1.0 - hit.score), // Client expects: lower = better
                rank: Some(i as u64),
                relevance: Some(hit.score), // Client expects: higher = better
            })
            .collect();

        Ok(Response::new(SearchResponse {
            result: Some(QueryResult { data: items }),
        }))
    }

    /// Health check endpoint.
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: "ok".to_string(),
        }))
    }

    // --- Workspace management methods ---

    /// Lists all workspaces for the authenticated user.
    async fn list_workspaces(
        &self,
        request: Request<ListWorkspacesRequest>,
    ) -> Result<Response<ListWorkspacesResponse>, Status> {
        let user_id = authenticate(&self.db, &request).await?;

        let rows = self
            .db
            .list_workspaces_for_user(&user_id)
            .await
            .into_status("Failed to list workspaces")?;

        let workspaces: Vec<Workspace> = rows.into_iter().map(workspace_row_to_proto).collect();

        Ok(Response::new(ListWorkspacesResponse { workspaces }))
    }

    /// Retrieves workspace info by ID.
    async fn get_workspace_info(
        &self,
        request: Request<GetWorkspaceInfoRequest>,
    ) -> Result<Response<GetWorkspaceInfoResponse>, Status> {
        let req_ref = request.get_ref();
        let workspace_id = extract_workspace_id(req_ref.workspace_id.clone())?;
        authenticate_and_verify_owner(&self.db, &request, &workspace_id).await?;

        let workspace = self
            .db
            .get_workspace(&workspace_id)
            .await
            .into_status("Failed to get workspace")?;

        Ok(Response::new(GetWorkspaceInfoResponse {
            workspace: workspace.map(workspace_row_to_proto),
        }))
    }

    /// Deletes a workspace, its Qdrant collection, and all SQLite metadata.
    async fn delete_workspace(
        &self,
        request: Request<DeleteWorkspaceRequest>,
    ) -> Result<Response<DeleteWorkspaceResponse>, Status> {
        let req_ref = request.get_ref();
        let workspace_id = extract_workspace_id(req_ref.workspace_id.clone())?;
        authenticate_and_verify_owner(&self.db, &request, &workspace_id).await?;

        // Delete Qdrant collection
        if let Err(e) = self.qdrant.delete_collection(&workspace_id).await {
            warn!(workspace_id = %workspace_id, error = ?e, "Failed to delete Qdrant collection, continuing");
        }

        // Delete from SQLite
        self.db
            .delete_workspace(&workspace_id)
            .await
            .into_status("Failed to delete workspace")?;

        info!(workspace_id = %workspace_id, "Workspace deleted");

        Ok(Response::new(DeleteWorkspaceResponse {
            workspace_id: Some(WorkspaceId { id: workspace_id }),
        }))
    }

    // --- Utility methods ---

    /// Validates file syntax. MVP: returns UnsupportedLanguage for all files.
    async fn validate_files(
        &self,
        request: Request<ValidateFilesRequest>,
    ) -> Result<Response<ValidateFilesResponse>, Status> {
        let req = request.into_inner();

        let results: Vec<FileValidationResult> = req
            .files
            .into_iter()
            .map(|file| FileValidationResult {
                file_path: file.path,
                status: Some(ValidationStatus {
                    status: Some(validation_status::Status::UnsupportedLanguage(
                        UnsupportedLanguage {},
                    )),
                }),
            })
            .collect();

        Ok(Response::new(ValidateFilesResponse { results }))
    }

    /// Fuzzy search: finds needle in haystack using case-insensitive substring matching.
    async fn fuzzy_search(
        &self,
        request: Request<FuzzySearchRequest>,
    ) -> Result<Response<FuzzySearchResponse>, Status> {
        let req = request.into_inner();
        let needle_lower = req.needle.to_lowercase();

        let mut matches: Vec<SearchMatch> = Vec::new();

        for (i, line) in req.haystack.lines().enumerate() {
            if line.to_lowercase().contains(&needle_lower) {
                let line_num = (i + 1) as u32; // 1-based
                matches.push(SearchMatch {
                    start_line: line_num,
                    end_line: line_num,
                });
                if !req.search_all {
                    break;
                }
            }
        }

        Ok(Response::new(FuzzySearchResponse { matches }))
    }

    // --- Stubs (not called by client) ---

    async fn chunk_files(
        &self,
        _request: Request<ChunkFilesRequest>,
    ) -> Result<Response<ChunkFilesResponse>, Status> {
        Err(Status::unimplemented("ChunkFiles is not supported"))
    }

    async fn select_skill(
        &self,
        _request: Request<SelectSkillRequest>,
    ) -> Result<Response<SelectSkillResponse>, Status> {
        Err(Status::unimplemented("SelectSkill is not supported"))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use super::compute_hash;

    #[test]
    fn test_compute_hash_matches_forge_client() {
        // Test vector: must match crates/forge_app/src/utils.rs compute_hash
        let content = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let actual = compute_hash(content);
        // SHA-256 of the above content as lowercase hex
        let expected = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(content.as_bytes());
            hex::encode(h.finalize())
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_compute_hash_empty_string() {
        let actual = compute_hash("");
        // SHA-256 of empty string
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(actual, expected);
    }
}
