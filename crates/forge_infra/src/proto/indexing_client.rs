use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use forge_app::IndexingClientInfra;
use forge_domain::{CodeSearchResult, IndexWorkspaceId, UploadStats, UserId as DomainUserId};
use tonic::transport::Channel;

// Include the generated proto code at module level
tonic::include_proto!("forge.v1");

use forge_service_client::ForgeServiceClient;

/// gRPC implementation of IndexingClientInfra
pub struct IndexingClient {
    client: ForgeServiceClient<Channel>,
}

impl IndexingClient {
    /// Create a new gRPC client connected to the given server URL
    ///
    /// # Errors
    /// Returns an error if connection fails
    pub async fn new(server_url: impl Into<String>) -> Result<Self> {
        let channel = Channel::from_shared(server_url.into())?.connect().await?;

        let client = ForgeServiceClient::new(channel);

        Ok(Self { client })
    }
}

#[async_trait]
impl IndexingClientInfra for IndexingClient {
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
            .id
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
        files: Vec<(PathBuf, String)>,
    ) -> Result<UploadStats> {
        let proto_files: Vec<File> = files
            .into_iter()
            .map(|(path, content)| File { path: path.to_string_lossy().to_string(), content })
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
    ) -> Result<Vec<CodeSearchResult>> {
        let request = tonic::Request::new(SearchRequest {
            user_id: Some(UserId { id: user_id.to_string() }),
            workspace_id: Some(WorkspaceId { id: workspace_id.to_string() }),
            query: Some(Query {
                query: Some(query.to_string()),
                limit: Some(limit as u32),
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
                        node_id,
                        file_path: file.path,
                        content: file.content,
                        similarity,
                    },
                    node_data::Kind::FileRef(file_ref) => {
                        CodeSearchResult::FileRef { node_id, file_path: file_ref.path, similarity }
                    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires running forge-ce server
    async fn test_create_workspace_and_upload() {
        let client = IndexingClient::new("http://localhost:8080").await.unwrap();

        let user_id = DomainUserId::generate();
        let path = PathBuf::from("/tmp/test");

        // Server returns the workspace_id
        let workspace_id = client.create_workspace(&user_id, &path).await.unwrap();

        let files = vec![(PathBuf::from("test.rs"), "fn main() {}".to_string())];

        client
            .upload_files(&user_id, &workspace_id, files)
            .await
            .unwrap();
    }
}
