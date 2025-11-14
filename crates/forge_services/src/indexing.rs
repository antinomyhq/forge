use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::usize;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_app::utils::format_display_path;
use forge_app::{
    EnvironmentInfra, FileReaderInfra, IndexingClientInfra, IndexingService, Walker, WalkerInfra,
    compute_hash,
};
use forge_domain::{IndexStats, IndexWorkspaceId, IndexingRepository, UserId};
use futures::future::join_all;
use tracing::{info, warn};

const DEFAULT_BATCH_SIZE: usize = 20;

/// Represents a file with its content and computed hash
struct IndexedFile {
    /// Relative path from the workspace root
    path: String,
    /// File content
    content: String,
    /// SHA-256 hash of the content
    hash: String,
}

impl IndexedFile {
    fn new(path: String, content: String, hash: String) -> Self {
        Self { path, content, hash }
    }
}

pub struct ForgeIndexingService<F> {
    infra: Arc<F>,
}

impl<F> ForgeIndexingService<F> {
    /// Creates a new indexing service with the provided infrastructure.
    ///
    /// # Arguments
    /// * `infra` - Composed infrastructure implementing all required traits
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    fn get_batch_size(&self) -> usize
    where
        F: EnvironmentInfra,
    {
        self.infra
            .get_env_var("FORGE_INDEX_BATCH_SIZE")
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_BATCH_SIZE)
    }

    /// Fetches server files, deletes outdated/orphaned ones, and returns
    /// current state.
    ///
    /// This method:
    /// 1. Fetches existing files from the server
    /// 2. Identifies files that are outdated (changed hash) or orphaned
    ///    (deleted locally)
    /// 3. Deletes those files from the server
    /// 4. Returns the server hashes for upload comparison
    ///
    /// # Arguments
    /// * `user_id` - The user ID
    /// * `workspace_id` - The workspace ID
    /// * `local_file_map` - Map of local file paths to their hashes
    ///
    /// # Errors
    /// Returns an error if deletion fails
    async fn sync_server_files(
        &self,
        user_id: &UserId,
        workspace_id: &IndexWorkspaceId,
        local_file_map: &HashMap<String, String>,
        workspace_root: &Path,
    ) -> Result<HashMap<String, String>>
    where
        F: IndexingClientInfra,
    {
        info!("Fetching existing file hashes from server to detect changes...");
        let server_hashes = self
            .infra
            .list_workspace_files(user_id, workspace_id)
            .await
            .map(|files| {
                let hashes: HashMap<_, _> = files
                    .into_iter()
                    .map(|f| {
                        // Normalize path to handle legacy absolute paths
                        let normalized_path =
                            format_display_path(&PathBuf::from(&f.path), workspace_root);
                        (normalized_path, f.hash)
                    })
                    .collect();
                info!("Found {} files on server", hashes.len());
                hashes
            })
            .unwrap_or_else(|e| {
                warn!(
                    "Failed to fetch existing files: {}. Will upload all files.",
                    e
                );
                HashMap::new()
            });

        // Identify outdated/orphaned files
        let files_to_delete: Vec<String> = server_hashes
            .iter()
            .filter(|(path, hash)| local_file_map.get(*path) != Some(*hash))
            .map(|(path, _)| path.clone())
            .collect();

        // Delete outdated/orphaned files from server
        if !files_to_delete.is_empty() {
            info!(
                "Deleting {} old/orphaned files from server before re-indexing",
                files_to_delete.len()
            );
            self.infra
                .delete_files(user_id, workspace_id, files_to_delete)
                .await
                .context("Failed to delete old/orphaned files")?;
        }

        Ok(server_hashes)
    }

    /// Determines which files need to be uploaded by comparing local and server
    /// state.
    async fn find_files_to_upload(
        &self,
        all_files: Vec<IndexedFile>,
        is_new_workspace: bool,
        user_id: &UserId,
        workspace_id: &IndexWorkspaceId,
        workspace_root: &Path,
    ) -> Result<Vec<(String, String)>>
    where
        F: IndexingRepository + IndexingClientInfra,
    {
        let total_file_count = all_files.len();

        // Build map of local files for comparison
        let local_file_map: HashMap<String, String> = all_files
            .iter()
            .map(|file| (file.path.clone(), file.hash.clone()))
            .collect();

        // Sync server files (fetch, delete outdated, return current state)
        let server_hashes = if is_new_workspace {
            HashMap::new()
        } else {
            self.sync_server_files(user_id, workspace_id, &local_file_map, workspace_root)
                .await?
        };

        // Identify files that need to be uploaded (new or changed)
        let files_to_upload: Vec<_> = all_files
            .into_iter()
            .filter_map(|file| {
                let needs_upload = server_hashes.get(&file.path) != Some(&file.hash);
                needs_upload.then_some((file.path, file.content))
            })
            .collect();

        // Log optimization stats
        if !server_hashes.is_empty() {
            let skipped = total_file_count - files_to_upload.len();
            info!(
                "Uploading {} changed files (skipping {} unchanged)",
                files_to_upload.len(),
                skipped
            );
        }

        Ok(files_to_upload)
    }

    /// Walks the directory, reads all files, and computes their hashes.
    ///
    /// # Arguments
    /// * `dir_path` - The directory path to index
    ///
    /// # Returns
    /// A vector of indexed files with their content and hashes
    ///
    /// # Errors
    /// Returns error if walking fails or no files are found
    async fn read_files(&self, dir_path: &Path) -> Result<Vec<IndexedFile>>
    where
        F: WalkerInfra + FileReaderInfra,
    {
        // Walk directory
        info!("Walking directory to discover files");
        let walker_config = Walker::conservative().cwd(dir_path.to_path_buf()).max_depth(usize::MAX).max_breadth(usize::MAX);
        let walked_files = self
            .infra
            .walk(walker_config)
            .await
            .context("Failed to walk directory")?
            .into_iter()
            .filter(|f| !f.is_dir())
            .collect::<Vec<_>>();

        info!(file_count = walked_files.len(), "Discovered files");
        anyhow::ensure!(!walked_files.is_empty(), "No files found to index");

        // Read all files and compute hashes
        let infra = self.infra.clone();
        let read_tasks = walked_files.into_iter().map(|walked| {
            let infra = infra.clone();
            let file_path = dir_path.join(&walked.path);
            let relative_path = walked.path.clone();

            async move {
                infra
                    .read_utf8(&file_path)
                    .await
                    .map(|content| {
                        let hash = compute_hash(&content);
                        IndexedFile::new(relative_path.clone(), content, hash)
                    })
                    .map_err(|e| {
                        warn!(path = %relative_path, error = %e, "Failed to read file");
                        e
                    })
                    .ok()
            }
        });

        let all_files: Vec<_> = join_all(read_tasks).await.into_iter().flatten().collect();
        Ok(all_files)
    }
}

#[async_trait]
impl<F: IndexingRepository + IndexingClientInfra + WalkerInfra + FileReaderInfra + EnvironmentInfra>
    IndexingService for ForgeIndexingService<F>
{
    async fn index(&self, path: PathBuf) -> Result<IndexStats> {
        info!(path = %path.display(), "Starting codebase indexing");

        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

        info!(canonical_path = %canonical_path.display(), "Resolved canonical path");

        let existing_workspace = self.infra.find_by_path(&canonical_path).await?;

        let (workspace_id, user_id, is_new_workspace) = existing_workspace
            .map(|workspace| (workspace.workspace_id, workspace.user_id, false))
            .unwrap_or_else(|| {
                let user_id = UserId::generate();
                (IndexWorkspaceId::generate(), user_id, true)
            });

        // Create workspace on server if new
        let workspace_id = if is_new_workspace {
            info!("Creating new workspace on server");
            self.infra
                .create_workspace(&user_id, &canonical_path)
                .await
                .context("Failed to create workspace on server")?
        } else {
            info!(workspace_id = %workspace_id, "Using existing workspace");
            workspace_id
        };

        // Read all files and compute hashes
        let all_files = self.read_files(&canonical_path).await?;
        let total_file_count = all_files.len();

        // Determine which files need to be uploaded
        let files_to_upload = self
            .find_files_to_upload(
                all_files,
                is_new_workspace,
                &user_id,
                &workspace_id,
                &canonical_path,
            )
            .await?;

        // Early exit if nothing to upload
        if files_to_upload.is_empty() {
            info!(
                "All {} files are up to date - nothing to upload",
                total_file_count
            );
            self.infra
                .upsert(&workspace_id, &user_id, &canonical_path)
                .await
                .context("Failed to save workspace")?;
            return Ok(IndexStats::new(
                workspace_id,
                total_file_count,
                forge_domain::UploadStats::default(),
            ));
        }

        // Upload in batches
        let batch_size = self.get_batch_size();
        let mut total_stats = forge_domain::UploadStats::default();

        for batch in files_to_upload.chunks(batch_size) {
            let stats = self
                .infra
                .upload_files(&user_id, &workspace_id, batch.to_vec())
                .await
                .context("Failed to upload files")?;
            total_stats = total_stats + stats;
        }

        // Save workspace metadata
        self.infra
            .upsert(&workspace_id, &user_id, &canonical_path)
            .await
            .context("Failed to save workspace")?;

        info!(
            workspace_id = %workspace_id,
            total_files = total_file_count,
            uploaded = files_to_upload.len(),
            "Indexing completed successfully"
        );

        Ok(IndexStats::new(workspace_id, total_file_count, total_stats))
    }

    /// Performs semantic code search on an indexed workspace.
    ///
    /// # Arguments
    /// * `path` - Workspace directory path (must be previously indexed)
    /// * `query` - Natural language search query
    /// * `limit` - Maximum number of results to return
    /// * `top_k` - Number of highest probability tokens to consider (1-1000)
    ///
    /// # Errors
    /// Returns error if:
    /// - Path is invalid or cannot be canonicalized
    /// - Workspace has not been indexed (suggests running `forge index .`)
    /// - Search request to indexing server fails
    async fn query(
        &self,
        path: PathBuf,
        query: &str,
        limit: usize,
        top_k: Option<u32>,
    ) -> Result<Vec<forge_domain::CodeSearchResult>> {
        // Step 1: Canonicalize path
        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

        // Step 2: Check if workspace is indexed
        let workspace = self
            .infra
            .find_by_path(&canonical_path)
            .await
            .context("Failed to query database")?
            .ok_or_else(|| anyhow::anyhow!("Workspace not indexed. Run `forge index .` first."))?;

        // Step 3: Search via indexing server
        let results = self
            .infra
            .search(
                &workspace.user_id,
                &workspace.workspace_id,
                query,
                limit,
                top_k,
            )
            .await
            .context("Failed to search")?;

        Ok(results)
    }

    /// Lists all indexed workspaces.
    ///
    /// Gets the user_id from any indexed workspace in the local database.
    /// If no workspaces exist locally, returns an empty list.
    ///
    /// # Errors
    /// Returns error if the request to indexing server fails.
    async fn list_indexes(&self) -> Result<Vec<forge_domain::WorkspaceInfo>> {
        // Get user_id from any indexed workspace
        let user_id =
            self.infra.as_ref().get_user_id().await?.ok_or_else(|| {
                anyhow::anyhow!("No workspaces indexed. Run `forge index` first.")
            })?;

        // List all workspaces for this user from indexing server
        self.infra
            .as_ref()
            .list_workspaces(&user_id)
            .await
            .context("Failed to list workspaces from indexing server")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_app::WalkedFile;
    use forge_domain::{
        CodeSearchResult, Environment, FileInfo, IndexWorkspaceId, IndexedWorkspace, UploadStats,
        UserId, WorkspaceInfo,
    };

    use super::*;

    struct MockInfra {
        existing_workspace: Option<IndexedWorkspace>,
        walked_files: Vec<WalkedFile>,
    }

    impl EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            Faker.fake()
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }
    }

    #[async_trait]
    impl IndexingRepository for MockInfra {
        async fn upsert(
            &self,
            _workspace_id: &IndexWorkspaceId,
            _user_id: &UserId,
            _path: &std::path::Path,
        ) -> Result<()> {
            Ok(())
        }

        async fn find_by_path(&self, _path: &std::path::Path) -> Result<Option<IndexedWorkspace>> {
            Ok(self.existing_workspace.clone())
        }

        async fn get_user_id(&self) -> Result<Option<UserId>> {
            Ok(self.existing_workspace.as_ref().map(|w| w.user_id.clone()))
        }
    }

    #[async_trait]
    impl IndexingClientInfra for MockInfra {
        async fn create_workspace(
            &self,
            _user_id: &UserId,
            _working_dir: &std::path::Path,
        ) -> Result<IndexWorkspaceId> {
            Ok(IndexWorkspaceId::generate())
        }

        async fn upload_files(
            &self,
            _user_id: &UserId,
            _workspace_id: &IndexWorkspaceId,
            _files: Vec<(String, String)>,
        ) -> Result<UploadStats> {
            Ok(UploadStats::new(10, 5))
        }

        async fn search(
            &self,
            _user_id: &UserId,
            _workspace_id: &IndexWorkspaceId,
            _query: &str,
            _limit: usize,
            _top_k: Option<u32>,
        ) -> Result<Vec<CodeSearchResult>> {
            Ok(vec![])
        }

        async fn list_workspaces(&self, _user_id: &UserId) -> Result<Vec<WorkspaceInfo>> {
            Ok(vec![])
        }

        async fn list_workspace_files(
            &self,
            _user_id: &UserId,
            _workspace_id: &IndexWorkspaceId,
        ) -> Result<Vec<forge_domain::FileHash>> {
            Ok(vec![])
        }

        async fn delete_files(
            &self,
            _user_id: &UserId,
            _workspace_id: &IndexWorkspaceId,
            _file_paths: Vec<String>,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl WalkerInfra for MockInfra {
        async fn walk(&self, _config: Walker) -> Result<Vec<WalkedFile>> {
            Ok(self.walked_files.clone())
        }
    }

    #[async_trait]
    impl FileReaderInfra for MockInfra {
        async fn read_utf8(&self, _path: &std::path::Path) -> Result<String> {
            Ok("fn main() {}".to_string())
        }

        async fn read(&self, _path: &std::path::Path) -> Result<Vec<u8>> {
            Ok(b"fn main() {}".to_vec())
        }

        async fn range_read_utf8(
            &self,
            _path: &std::path::Path,
            _start_line: u64,
            _end_line: u64,
        ) -> Result<(String, FileInfo)> {
            Ok((
                "fn main() {}".to_string(),
                FileInfo { total_lines: 1, start_line: 1, end_line: 1 },
            ))
        }
    }

    #[tokio::test]
    async fn test_index_new_workspace() {
        let fixture = Arc::new(MockInfra {
            existing_workspace: None,
            walked_files: vec![WalkedFile {
                path: "test.rs".to_string(),
                file_name: Some("test.rs".to_string()),
                size: 100,
            }],
        });

        let service = ForgeIndexingService::new(fixture);
        let actual = service.index(PathBuf::from("/tmp/forge-test-index")).await;

        assert!(actual.is_ok());
        let stats = actual.unwrap();
        assert_eq!(stats.files_processed, 1);
        assert_eq!(stats.upload_stats.nodes_created, 10);
        assert_eq!(stats.upload_stats.relations_created, 5);
    }

    #[tokio::test]
    async fn test_index_existing_workspace() {
        let workspace_id = IndexWorkspaceId::generate();
        let user_id = UserId::generate();

        let fixture = Arc::new(MockInfra {
            existing_workspace: Some(IndexedWorkspace {
                workspace_id: workspace_id.clone(),
                user_id: user_id.clone(),
                path: PathBuf::from("/tmp/forge-test-index"),
                created_at: chrono::Utc::now(),
                updated_at: None,
            }),
            walked_files: vec![WalkedFile {
                path: "test.rs".to_string(),
                file_name: Some("test.rs".to_string()),
                size: 100,
            }],
        });

        let service = ForgeIndexingService::new(fixture);
        let actual = service.index(PathBuf::from("/tmp/forge-test-index")).await;

        assert!(actual.is_ok());
        let stats = actual.unwrap();
        assert_eq!(stats.workspace_id, workspace_id);
    }

    #[tokio::test]
    async fn test_index_no_files() {
        let fixture = Arc::new(MockInfra { existing_workspace: None, walked_files: vec![] });

        let service = ForgeIndexingService::new(fixture);
        let actual = service.index(PathBuf::from("/tmp/forge-test-index")).await;

        assert!(actual.is_err());
        let error_msg = actual.unwrap_err().to_string();
        assert!(error_msg.contains("No files found"));
    }
}
