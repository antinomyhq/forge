use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use forge_domain::{
    ApiKey, CodeSearchResult, ContextEngineRepository, IndexingAuth, UploadStats, UserId,
    WorkspaceId, WorkspaceInfo,
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

/// gRPC implementation of CodebaseRepository
pub struct ForgeContextEngineRepository {
    client: ForgeServiceClient<Channel>,
}

impl ForgeContextEngineRepository {
    /// Create a new gRPC client with lazy connection
    pub fn new(server_url: impl Into<String>) -> Result<Self> {
        let channel = Channel::from_shared(server_url.into())?.connect_lazy();
        let client = ForgeServiceClient::new(channel);
        Ok(Self { client })
    }
}

#[async_trait]
impl ContextEngineRepository for ForgeContextEngineRepository {
    async fn authenticate(&self) -> Result<IndexingAuth> {
        let mut client = self.client.clone();

        let request = tonic::Request::new(CreateApiKeyRequest { user_id: None });

        let response = client
            .create_api_key(request)
            .await
            .context("Failed to call CreateApiKey gRPC")?
            .into_inner();

        let user_id = response.user_id.context("Missing user_id in response")?.id;
        let user_id = UserId::from_string(&user_id).context("Invalid user_id returned from API")?;

        let token: ApiKey = response.key.into();

        Ok(IndexingAuth { user_id, token, created_at: Utc::now() })
    }

    async fn create_workspace(
        &self,
        working_dir: &std::path::Path,
        auth_token: &forge_domain::ApiKey,
    ) -> Result<WorkspaceId> {
        let mut request = tonic::Request::new(CreateWorkspaceRequest {
            workspace: Some(WorkspaceDefinition {
                working_dir: working_dir.to_string_lossy().to_string(),
            }),
        });

        // Add authorization header
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", &**auth_token).parse()?,
        );

        let mut client = self.client.clone();
        let response = client.create_workspace(request).await?;

        let workspace = response
            .into_inner()
            .workspace
            .context("No workspace in response")?;

        let workspace_id = workspace
            .workspace_id
            .context("Server did not return workspace ID in CreateWorkspace response")?
            .id;

        WorkspaceId::from_string(&workspace_id)
            .context("Failed to parse workspace ID from server response")
    }

    async fn upload_files(
        &self,
        upload: &forge_domain::FileUpload,
        auth_token: &forge_domain::ApiKey,
    ) -> Result<UploadStats> {
        let files: Vec<File> = upload
            .data
            .iter()
            .map(|file_read| File {
                path: file_read.path.clone(),
                content: file_read.content.clone(),
            })
            .collect();

        let mut request = tonic::Request::new(UploadFilesRequest {
            workspace_id: Some(proto_generated::WorkspaceId {
                id: upload.workspace_id.to_string(),
            }),
            content: Some(FileUploadContent { files, git: None }),
        });

        // Add authorization header
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", &**auth_token).parse()?,
        );

        let mut client = self.client.clone();
        let response = client.upload_files(request).await?;

        let result = response
            .into_inner()
            .result
            .context("Server did not return upload result in UploadFiles response")?;

        Ok(UploadStats::new(result.nodes.len(), result.relations.len()))
    }

    /// Search for code using semantic search
    async fn search(
        &self,
        search_query: &forge_domain::CodeSearchQuery<'_>,
        auth_token: &forge_domain::ApiKey,
    ) -> Result<Vec<CodeSearchResult>> {
        let mut request = tonic::Request::new(SearchRequest {
            workspace_id: Some(proto_generated::WorkspaceId {
                id: search_query.workspace_id.to_string(),
            }),
            query: Some(Query {
                prompt: Some(search_query.data.query.to_string()),
                limit: Some(search_query.data.limit as u32),
                top_k: search_query.data.top_k,
                ..Default::default()
            }),
        });

        // Add authorization header
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", &**auth_token).parse()?,
        );

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
    async fn list_workspaces(
        &self,
        auth_token: &forge_domain::ApiKey,
    ) -> Result<Vec<WorkspaceInfo>> {
        let mut request = tonic::Request::new(ListWorkspacesRequest {});

        // Add authorization header
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", &**auth_token).parse()?,
        );

        let mut client = self.client.clone();
        let response = client.list_workspaces(request).await?;

        let workspaces = response
            .into_inner()
            .workspaces
            .into_iter()
            .filter_map(|workspace| {
                let id_msg = workspace.workspace_id?;
                let workspace_id = WorkspaceId::from_string(&id_msg.id).ok()?;
                Some(WorkspaceInfo { workspace_id, working_dir: workspace.working_dir })
            })
            .collect();

        Ok(workspaces)
    }

    /// List all files in a workspace with their hashes
    async fn list_workspace_files(
        &self,
        workspace: &forge_domain::WorkspaceFiles,
        auth_token: &forge_domain::ApiKey,
    ) -> Result<Vec<forge_domain::FileHash>> {
        let mut request = tonic::Request::new(ListFilesRequest {
            workspace_id: Some(proto_generated::WorkspaceId {
                id: workspace.workspace_id.to_string(),
            }),
        });

        // Add authorization header
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", &**auth_token).parse()?,
        );

        let mut client = self.client.clone();
        let response = client.list_files(request).await?;

        // Extract file paths and hashes from FileRefNode
        let files = response
            .into_inner()
            .files
            .into_iter()
            .filter_map(|file_ref_node| {
                let data = file_ref_node.data?;
                Some(forge_domain::FileHash { path: data.path, hash: data.file_hash })
            })
            .collect();

        Ok(files)
    }

    /// Delete files from a workspace
    async fn delete_files(
        &self,
        deletion: &forge_domain::FileDeletion,
        auth_token: &forge_domain::ApiKey,
    ) -> Result<()> {
        if deletion.data.is_empty() {
            return Ok(());
        }

        let mut request = tonic::Request::new(DeleteFilesRequest {
            workspace_id: Some(proto_generated::WorkspaceId {
                id: deletion.workspace_id.to_string(),
            }),
            file_paths: deletion.data.clone(),
        });

        // Add authorization header
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", &**auth_token).parse()?,
        );

        let mut client = self.client.clone();
        client.delete_files(request).await?;

        Ok(())
    }

    async fn delete_workspace(
        &self,
        workspace_id: &forge_domain::WorkspaceId,
        auth_token: &forge_domain::ApiKey,
    ) -> Result<()> {
        let mut request = tonic::Request::new(DeleteWorkspaceRequest {
            workspace_id: Some(proto_generated::WorkspaceId { id: workspace_id.to_string() }),
        });

        // Add authorization header
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", &**auth_token).parse()?,
        );

        let mut client = self.client.clone();
        client.delete_workspace(request).await?;

        Ok(())
    }
}
