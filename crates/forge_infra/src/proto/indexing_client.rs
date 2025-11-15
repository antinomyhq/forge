use anyhow::Result;
use async_trait::async_trait;
use forge_app::CodebaseRepository;
use forge_domain::{
    CodeSearchResult, IndexWorkspaceId, UploadStats, UserId as DomainUserId, WorkspaceInfo,
};
use tonic::transport::Channel;

// Include the generated proto code at module level
// Allow dead code since protobuf generates code that may not be fully used
#[allow(dead_code)]
mod proto_generated {
    tonic::include_proto!("forge.v1");
}

use forge_service_client::ForgeServiceClient;
use proto_generated::*;

/// gRPC implementation of IndexingClientInfra
pub struct IndexingClient {
    client: ForgeServiceClient<Channel>,
}

impl IndexingClient {
    /// Create a new gRPC client with lazy connection
    pub fn new(server_url: impl Into<String>) -> Result<Self> {
        let channel = Channel::from_shared(server_url.into())?.connect_lazy();
        let client = ForgeServiceClient::new(channel);
        Ok(Self { client })
    }
}

#[async_trait]
impl CodebaseRepository for IndexingClient {
    async fn create_workspace(
        &self,
        user_id: &DomainUserId,
        working_dir: &std::path::Path,
    ) -> Result<IndexWorkspaceId> {
        let request = CreateWorkspaceRequest {
            user_id: Some(UserId { id: user_id.to_string() }),
            workspace: Some(WorkspaceDefinition {
                working_dir: working_dir.to_string_lossy().to_string(),
            }),
        };

        let mut client = self.client.clone();
        let response = client.create_workspace(request).await?;

        let workspace = response
            .into_inner()
            .workspace
            .ok_or_else(|| anyhow::anyhow!("No workspace in response"))?;

        let workspace_id = workspace
            .workspace_id
            .ok_or_else(|| {
                anyhow::anyhow!("Server did not return workspace ID in CreateWorkspace response")
            })?
            .id;

        IndexWorkspaceId::from_string(&workspace_id)
    }

    async fn upload_files(
        &self,
        user_id: &DomainUserId,
        workspace_id: &IndexWorkspaceId,
        files: Vec<forge_domain::FileRead>,
    ) -> Result<UploadStats> {
        let proto_files: Vec<File> = files
            .into_iter()
            .map(|file_read| File { path: file_read.path, content: file_read.content })
            .collect();

        let request = UploadFilesRequest {
            user_id: Some(UserId { id: user_id.to_string() }),
            workspace_id: Some(WorkspaceId { id: workspace_id.to_string() }),
            content: Some(FileUploadContent { files: proto_files, git: None }),
        };

        let mut client = self.client.clone();
        let response = client.upload_files(request).await?;

        let result = response.into_inner().result.ok_or_else(|| {
            anyhow::anyhow!("Server did not return upload result in UploadFiles response")
        })?;

        Ok(UploadStats::new(result.nodes.len(), result.relations.len()))
    }

    /// Search for code using semantic search
    async fn search(
        &self,
        user_id: &DomainUserId,
        workspace_id: &IndexWorkspaceId,
        query: &str,
        limit: usize,
        top_k: Option<u32>,
    ) -> Result<Vec<CodeSearchResult>> {
        let request = tonic::Request::new(SearchRequest {
            user_id: Some(UserId { id: user_id.to_string() }),
            workspace_id: Some(WorkspaceId { id: workspace_id.to_string() }),
            query: Some(Query {
                prompt: Some(query.to_string()),
                limit: Some(limit as u32),
                top_k,
                ..Default::default()
            }),
        });

        let mut client = self.client.clone();
        let response = client.search(request).await?;

        let result = response.into_inner().result.unwrap_or_default();

        // Convert QueryItems to CodeSearchResults
        let results = result
            .data
            .into_iter()
            .filter_map(|query_item| {
                let node = query_item.node?;
                let node_data = node.data?;
                let node_id = node.node_id.map(|n| n.id).unwrap_or_default();
                let similarity = query_item.distance.unwrap_or(0.0);

                // Convert proto node to domain CodeSearchResult based on type
                let result = match node_data.kind? {
                    node_data::Kind::FileChunk(chunk) => CodeSearchResult::FileChunk {
                        node_id,
                        file_path: chunk.path,
                        content: chunk.content,
                        start_line: chunk.start_line,
                        end_line: chunk.end_line,
                        similarity,
                    },
                    node_data::Kind::File(file) => CodeSearchResult::File {
                        node_id: node_id.clone(),
                        file_path: file.path,
                        content: file.content,
                        hash: node.hash,
                        similarity,
                    },
                    node_data::Kind::FileRef(file_ref) => CodeSearchResult::FileRef {
                        node_id,
                        file_path: file_ref.path,
                        file_hash: file_ref.file_hash,
                        similarity,
                    },
                    node_data::Kind::Note(note) => {
                        CodeSearchResult::Note { node_id, content: note.content, similarity }
                    }
                    node_data::Kind::Task(task) => {
                        CodeSearchResult::Task { node_id, task: task.task, similarity }
                    }
                };

                Some(result)
            })
            .collect();

        Ok(results)
    }

    /// List all workspaces for a user
    async fn list_workspaces(&self, user_id: &DomainUserId) -> Result<Vec<WorkspaceInfo>> {
        let request = tonic::Request::new(ListWorkspacesRequest {
            user_id: Some(UserId { id: user_id.to_string() }),
        });

        let mut client = self.client.clone();
        let response = client.list_workspaces(request).await?;

        let workspaces = response
            .into_inner()
            .workspaces
            .into_iter()
            .filter_map(|workspace| {
                let id_msg = workspace.workspace_id?;
                let workspace_id = IndexWorkspaceId::from_string(&id_msg.id).ok()?;
                Some(WorkspaceInfo { workspace_id, working_dir: workspace.working_dir })
            })
            .collect();

        Ok(workspaces)
    }

    /// List all files in a workspace with their hashes
    async fn list_workspace_files(
        &self,
        user_id: &DomainUserId,
        workspace_id: &IndexWorkspaceId,
    ) -> Result<Vec<forge_domain::FileHash>> {
        let request = tonic::Request::new(ListFilesRequest {
            user_id: Some(UserId { id: user_id.to_string() }),
            workspace_id: Some(WorkspaceId { id: workspace_id.to_string() }),
        });

        let mut client = self.client.clone();
        let response = client.list_files(request).await?;

        // Extract file paths and hashes from FileRefNode
        let files = response
            .into_inner()
            .files
            .into_iter()
            .filter_map(|file_ref_node| {
                let data = file_ref_node.data?;
                Some(forge_domain::FileHash { path: data.path, hash: file_ref_node.hash })
            })
            .collect();

        Ok(files)
    }

    /// Delete files from a workspace
    async fn delete_files(
        &self,
        user_id: &DomainUserId,
        workspace_id: &IndexWorkspaceId,
        file_paths: Vec<String>,
    ) -> Result<()> {
        if file_paths.is_empty() {
            return Ok(());
        }

        let request = tonic::Request::new(DeleteFilesRequest {
            user_id: Some(UserId { id: user_id.to_string() }),
            workspace_id: Some(WorkspaceId { id: workspace_id.to_string() }),
            file_paths,
        });

        let mut client = self.client.clone();
        client.delete_files(request).await?;

        Ok(())
    }
}
