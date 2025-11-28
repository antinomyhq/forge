use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_app::{ContextEngineService, FileReaderInfra, Walker, WalkerInfra, compute_hash};
use forge_domain::{
    AuthCredential, ContextEngineRepository, FileUploadResponse, ProviderId, ProviderRepository,
    UserId, WorkspaceId, WorkspaceRepository,
};
use futures::future::join_all;
use tracing::{info, warn};

/// Represents a file with its content and computed hash
#[derive(Debug)]
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

/// Service for indexing codebases and performing semantic search
pub struct ForgeIndexingService<F> {
    infra: Arc<F>,
}

impl<F> ForgeIndexingService<F> {
    /// Creates a new indexing service with the provided infrastructure.
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    /// Extract WorkspaceAuth components from AuthCredential
    ///
    /// # Errors
    /// Returns an error if the credential is not an API key or contains invalid
    /// data
    fn extract_workspace_auth(
        credential: &AuthCredential,
    ) -> Result<(forge_domain::ApiKey, UserId)> {
        use forge_domain::AuthDetails;

        match &credential.auth_details {
            AuthDetails::ApiKey(token) => {
                // Extract user_id from URL params
                let user_id_str = credential
                    .url_params
                    .get(&"user_id".to_string().into())
                    .ok_or_else(|| {
                        anyhow::anyhow!("Missing user_id in ForgeServices credential")
                    })?;
                let user_id = UserId::from_string(user_id_str.as_str())?;

                Ok((token.clone(), user_id))
            }
            _ => anyhow::bail!("ForgeServices credential must be an API key"),
        }
    }

    /// Convert WorkspaceAuth to AuthCredential for storage
    fn workspace_auth_to_credential(auth: &forge_domain::WorkspaceAuth) -> AuthCredential {
        use std::collections::HashMap;

        let mut url_params = HashMap::new();
        url_params.insert(
            "user_id".to_string().into(),
            auth.user_id.to_string().into(),
        );

        AuthCredential {
            id: ProviderId::FORGE_SERVICES,
            auth_details: auth.clone().into(),
            url_params,
        }
    }

    /// Fetches server files, deletes outdated/orphaned ones, and returns
    /// current state.
    /// This method:
    /// 1. Fetches existing files from the server
    /// 2. Identifies files that are outdated (changed hash) or orphaned
    ///    (deleted locally)
    /// 3. Deletes those files from the server
    /// 4. Returns the server hashes for upload comparison
    async fn sync_server_files(
        &self,
        user_id: &UserId,
        workspace_id: &WorkspaceId,
        local_file_map: &HashMap<String, String>,
        auth_token: &forge_domain::ApiKey,
    ) -> Result<HashMap<String, String>>
    where
        F: ContextEngineRepository,
    {
        info!("Fetching existing file hashes from server to detect changes...");
        let workspace_files =
            forge_domain::CodeBase::new(user_id.clone(), workspace_id.clone(), ());
        let server_hashes = self
            .infra
            .list_workspace_files(&workspace_files, auth_token)
            .await
            .map(|files| {
                let hashes: HashMap<_, _> = files.into_iter().map(|f| (f.path, f.hash)).collect();
                info!("Found {} files on server", hashes.len());
                hashes
            })
            .unwrap_or_default();

        // Identify outdated/orphaned files
        let files_to_delete: Vec<String> = server_hashes
            .iter()
            .filter(|(path, hash)| local_file_map.get(*path) != Some(*hash))
            .map(|(path, _)| path.clone())
            .collect();

        // Delete outdated/orphaned files from server
        if !files_to_delete.is_empty() {
            info!(
                "Deleting {} old/orphaned files from server before syncing",
                files_to_delete.len()
            );
            let deletion =
                forge_domain::CodeBase::new(user_id.clone(), workspace_id.clone(), files_to_delete);
            self.infra
                .delete_files(&deletion, auth_token)
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
        workspace_id: &WorkspaceId,
        auth_token: &forge_domain::ApiKey,
    ) -> Result<Vec<(String, String)>>
    where
        F: WorkspaceRepository + ContextEngineRepository,
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
            self.sync_server_files(user_id, workspace_id, &local_file_map, auth_token)
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
    async fn read_files(&self, dir_path: &Path) -> Result<Vec<IndexedFile>>
    where
        F: WalkerInfra + FileReaderInfra,
    {
        // Walk directory
        info!("Walking directory to discover files");
        let mut walker_config = Walker::conservative()
            .cwd(dir_path.to_path_buf())
            .max_depth(usize::MAX)
            .max_breadth(usize::MAX)
            .max_files(usize::MAX)
            .skip_binary(true);
        walker_config.max_file_size = None;
        walker_config.max_total_size = None;

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

        // Filter binary files and read all text files
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
impl<
    F: WorkspaceRepository
        + ProviderRepository
        + ContextEngineRepository
        + WalkerInfra
        + FileReaderInfra,
> ContextEngineService for ForgeIndexingService<F>
{
    async fn sync_codebase(&self, path: PathBuf, batch_size: usize) -> Result<FileUploadResponse> {
        info!(path = %path.display(), "Starting codebase sync");

        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

        info!(canonical_path = %canonical_path.display(), "Resolved canonical path");

        // Get auth token (must already exist - caller should call ensure_auth first)
        let credential = self
            .infra
            .get_credential(&ProviderId::FORGE_SERVICES)
            .await?
            .context("No authentication credentials found. Please authenticate first.")?;

        let (token, user_id) = Self::extract_workspace_auth(&credential)?;

        let existing_workspace = self.infra.find_by_path(&canonical_path).await?;

        let (workspace_id, is_new_workspace) = match existing_workspace {
            Some(workspace) if workspace.user_id == user_id => (workspace.workspace_id, false),
            Some(workspace) => {
                if let Err(e) = self.infra.delete(&workspace.workspace_id).await {
                    warn!(error = %e, "Failed to delete old workspace entry from local database");
                }
                (WorkspaceId::generate(), true)
            }
            None => (WorkspaceId::generate(), true),
        };

        let workspace_id = if is_new_workspace {
            self.infra
                .create_workspace(&canonical_path, &token)
                .await
                .context("Failed to create workspace on server")?
        } else {
            workspace_id
        };

        // Read all files and compute hashes
        let all_files = self.read_files(&canonical_path).await?;
        let total_file_count = all_files.len();

        // Determine which files need to be uploaded
        let files_to_upload = self
            .find_files_to_upload(all_files, is_new_workspace, &user_id, &workspace_id, &token)
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
            return Ok(FileUploadResponse::new(
                workspace_id,
                total_file_count,
                forge_domain::FileUploadInfo::default(),
            )
            .is_new_workspace(is_new_workspace));
        }

        // Upload in batches
        let mut total_stats = forge_domain::FileUploadInfo::default();

        for batch in files_to_upload.chunks(batch_size) {
            let file_reads: Vec<forge_domain::FileRead> = batch
                .iter()
                .map(|(path, content)| forge_domain::FileRead::new(path.clone(), content.clone()))
                .collect();

            let upload =
                forge_domain::CodeBase::new(user_id.clone(), workspace_id.clone(), file_reads);

            let stats = self
                .infra
                .upload_files(&upload, &token)
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
            "Sync completed successfully"
        );

        Ok(
            FileUploadResponse::new(workspace_id, total_file_count, total_stats)
                .is_new_workspace(is_new_workspace),
        )
    }

    /// Performs semantic code search on a workspace.
    async fn query_codebase(
        &self,
        path: PathBuf,
        params: forge_domain::SearchParams<'_>,
    ) -> Result<Vec<forge_domain::CodeSearchResult>> {
        // Step 1: Canonicalize path
        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

        // Step 2: Check if workspace exists
        let workspace = self
            .infra
            .find_by_path(&canonical_path)
            .await
            .context("Failed to query database")?
            .ok_or(forge_domain::Error::WorkspaceNotFound)?;

        // Step 3: Get auth token
        let credential = self
            .infra
            .get_credential(&ProviderId::FORGE_SERVICES)
            .await?
            .ok_or(forge_domain::Error::AuthTokenNotFound)?;

        let (token, _) = Self::extract_workspace_auth(&credential)?;

        // Step 4: Search the codebase
        let search_query = forge_domain::CodeBase::new(
            workspace.user_id.clone(),
            workspace.workspace_id.clone(),
            params,
        );

        let results = self
            .infra
            .search(&search_query, &token)
            .await
            .context("Failed to search")?;

        Ok(results)
    }

    /// Lists all workspaces.
    async fn list_codebase(&self) -> Result<Vec<forge_domain::WorkspaceInfo>> {
        // Get auth token
        let credential = self
            .infra
            .get_credential(&ProviderId::FORGE_SERVICES)
            .await?
            .ok_or(forge_domain::Error::AuthTokenNotFound)?;

        let (token, _) = Self::extract_workspace_auth(&credential)?;

        // List all workspaces for this user
        self.infra
            .as_ref()
            .list_workspaces(&token)
            .await
            .context("Failed to list workspaces")
    }

    /// Retrieves workspace information for a specific path.
    async fn get_workspace_info(&self, path: PathBuf) -> Result<Option<forge_domain::WorkspaceInfo>>
    where
        F: WorkspaceRepository + ContextEngineRepository + ProviderRepository,
    {
        let path = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

        // Get auth token
        let credential = self
            .infra
            .get_credential(&ProviderId::FORGE_SERVICES)
            .await?
            .ok_or(forge_domain::Error::AuthTokenNotFound)?;

        let (token, _) = Self::extract_workspace_auth(&credential)?;

        // Find workspace by path
        let workspace = self.infra.find_by_path(&path).await?;

        if let Some(workspace) = workspace {
            // Get detailed workspace info from server
            self.infra
                .as_ref()
                .get_workspace(&workspace.workspace_id, &token)
                .await
                .context("Failed to get workspace info")
        } else {
            Ok(None)
        }
    }

    /// Deletes a workspace from both the server and local database.
    async fn delete_codebase(&self, workspace_id: &forge_domain::WorkspaceId) -> Result<()> {
        // Get auth token
        let credential = self
            .infra
            .get_credential(&ProviderId::FORGE_SERVICES)
            .await?
            .ok_or(forge_domain::Error::AuthTokenNotFound)?;

        let (token, _) = Self::extract_workspace_auth(&credential)?;

        // Delete from server
        self.infra
            .as_ref()
            .delete_workspace(workspace_id, &token)
            .await
            .context("Failed to delete workspace from server")?;

        // Delete from local database
        self.infra
            .as_ref()
            .delete(workspace_id)
            .await
            .context("Failed to delete workspace from local database")?;

        Ok(())
    }

    async fn is_indexed(&self, path: &std::path::Path) -> Result<bool> {
        // Canonicalize path first to ensure consistent comparison
        let canonical_path = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => return Ok(false), // Path doesn't exist, so it can't be indexed
        };

        // Check if workspace is indexed
        Ok(self
            .infra
            .as_ref()
            .find_by_path(&canonical_path)
            .await?
            .is_some())
    }

    async fn is_authenticated(&self) -> Result<bool> {
        Ok(self
            .infra
            .get_credential(&ProviderId::FORGE_SERVICES)
            .await?
            .is_some())
    }

    async fn create_auth_credentials(&self) -> Result<forge_domain::WorkspaceAuth> {
        // Authenticate with the indexing service
        let auth = self
            .infra
            .authenticate()
            .await
            .context("Failed to authenticate with indexing service")?;

        // Convert to AuthCredential and store
        let credential = Self::workspace_auth_to_credential(&auth);
        self.infra
            .upsert_credential(credential)
            .await
            .context("Failed to store authentication credentials")?;

        Ok(auth)
    }
}

// Additional authentication methods for ForgeIndexingService
impl<F> ForgeIndexingService<F>
where
    F: ProviderRepository + WorkspaceRepository + ContextEngineRepository,
{
    /// Login to the indexing service by storing an authentication token
    ///
    /// Authenticate with the indexing service and store credentials
    ///
    /// This method authenticates with the indexing service backend and stores
    /// the authentication credentials locally for future use.
    ///
    /// # Errors
    /// Returns an error if authentication or storing credentials fails
    pub async fn login(&self) -> Result<()> {
        // Call gRPC API to authenticate
        let auth = self
            .infra
            .authenticate()
            .await
            .context("Failed to authenticate with indexing service")?;

        // Convert to AuthCredential and store in credential.json
        let credential = Self::workspace_auth_to_credential(&auth);
        self.infra
            .upsert_credential(credential)
            .await
            .context("Failed to store authentication credentials")?;

        info!("Successfully logged in to indexing service");
        Ok(())
    }

    /// Logout from the indexing service by removing the authentication token
    ///
    /// # Errors
    /// Returns an error if deletion fails
    pub async fn logout(&self) -> Result<()> {
        self.infra
            .remove_credential(&ProviderId::FORGE_SERVICES)
            .await
            .context("Failed to logout from indexing service")?;

        info!("Successfully logged out from indexing service");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_app::WalkedFile;
    use forge_domain::{
        ApiKey, CodeSearchQuery, CodeSearchResult, FileDeletion, FileHash, FileInfo, FileUpload,
        FileUploadInfo, UserId, Workspace, WorkspaceAuth, WorkspaceFiles, WorkspaceId,
        WorkspaceInfo,
    };
    use pretty_assertions::assert_eq;

    use super::*;

    #[derive(Default, Clone)]
    struct MockInfra {
        files: HashMap<String, String>,
        workspace: Option<Workspace>,
        search_results: Vec<CodeSearchResult>,
        workspaces: Arc<tokio::sync::Mutex<Vec<WorkspaceInfo>>>,
        server_files: Vec<FileHash>,
        deleted_files: Arc<tokio::sync::Mutex<Vec<String>>>,
        uploaded_files: Arc<tokio::sync::Mutex<Vec<String>>>,
        authenticated: bool, // Track whether user is authenticated
    }

    impl MockInfra {
        /// New workspace that hasn't been indexed yet
        fn new(files: &[&str]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|p| (p.to_string(), format!("content of {}", p)))
                    .collect(),
                authenticated: true, // Simulate authenticated user
                ..Default::default()
            }
        }

        /// Indexed workspace where local and server are in sync (no changes)
        fn synced(files: &[&str]) -> Self {
            let files_map: HashMap<_, _> = files
                .iter()
                .map(|p| (p.to_string(), format!("content of {}", p)))
                .collect();
            let server_files = files
                .iter()
                .map(|p| FileHash {
                    path: p.to_string(),
                    hash: compute_hash(&format!("content of {}", p)),
                })
                .collect();

            Self {
                files: files_map,
                workspace: Some(workspace()),
                server_files,
                authenticated: true, // Simulate authenticated user
                ..Default::default()
            }
        }

        /// Indexed workspace where local and server are out of sync
        fn out_of_sync(local_files: &[&str], server_files: &[&str]) -> Self {
            let files_map: HashMap<_, _> = local_files
                .iter()
                .map(|p| (p.to_string(), format!("content of {}", p)))
                .collect();
            let server = server_files
                .iter()
                .map(|p| FileHash {
                    path: p.to_string(),
                    hash: compute_hash(&format!("content of {}", p)),
                })
                .collect();

            Self {
                files: files_map,
                workspace: Some(workspace()),
                server_files: server,
                authenticated: true, // Simulate authenticated user
                ..Default::default()
            }
        }
    }

    fn workspace() -> Workspace {
        Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: UserId::generate(),
            path: PathBuf::from("."),
            created_at: chrono::Utc::now(),
            updated_at: None,
        }
    }

    fn search_result() -> CodeSearchResult {
        CodeSearchResult {
            node: forge_domain::CodeNode::FileChunk {
                node_id: "n1".into(),
                file_path: "main.rs".into(),
                content: "fn main() {}".into(),
                start_line: 1,
                end_line: 1,
            },
            similarity: 0.95,
        }
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockInfra {
        async fn get_all_providers(&self) -> Result<Vec<forge_domain::AnyProvider>> {
            Ok(vec![])
        }

        async fn get_provider(&self, _id: ProviderId) -> Result<forge_domain::Provider<url::Url>> {
            unimplemented!("Not needed for indexing tests")
        }

        async fn upsert_credential(&self, _credential: AuthCredential) -> Result<()> {
            Ok(())
        }

        async fn get_credential(&self, id: &ProviderId) -> Result<Option<AuthCredential>> {
            if *id == ProviderId::FORGE_SERVICES && self.authenticated {
                let user_id = self
                    .workspace
                    .as_ref()
                    .map(|w| w.user_id.clone())
                    .unwrap_or_else(UserId::generate);

                let mut url_params = std::collections::HashMap::new();
                url_params.insert("user_id".to_string().into(), user_id.to_string().into());

                Ok(Some(AuthCredential {
                    id: ProviderId::FORGE_SERVICES,
                    auth_details: forge_domain::AuthDetails::ApiKey(
                        "test_token".to_string().into(),
                    ),
                    url_params,
                }))
            } else {
                Ok(None)
            }
        }

        async fn remove_credential(&self, _id: &ProviderId) -> Result<()> {
            Ok(())
        }

        async fn migrate_env_credentials(&self) -> Result<Option<forge_domain::MigrationResult>> {
            Ok(None)
        }
    }

    #[async_trait]
    impl WorkspaceRepository for MockInfra {
        async fn upsert(&self, _: &WorkspaceId, _: &UserId, _: &Path) -> Result<()> {
            Ok(())
        }
        async fn find_by_path(&self, _: &Path) -> Result<Option<Workspace>> {
            Ok(self.workspace.clone())
        }
        async fn get_user_id(&self) -> Result<Option<UserId>> {
            Ok(self.workspace.as_ref().map(|w| w.user_id.clone()))
        }
        async fn delete(&self, _: &WorkspaceId) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl ContextEngineRepository for MockInfra {
        async fn authenticate(&self) -> Result<WorkspaceAuth> {
            // Mock authentication - return fake user_id and token
            Ok(WorkspaceAuth::new(
                UserId::generate(),
                "test_token".to_string().into(),
            ))
        }

        async fn create_workspace(&self, _: &Path, _: &ApiKey) -> Result<WorkspaceId> {
            Ok(WorkspaceId::generate())
        }
        async fn upload_files(&self, upload: &FileUpload, _: &ApiKey) -> Result<FileUploadInfo> {
            self.uploaded_files
                .lock()
                .await
                .extend(upload.data.iter().map(|f| f.path.clone()));
            Ok(FileUploadInfo::new(upload.data.len(), upload.data.len()))
        }
        async fn search(
            &self,
            _: &CodeSearchQuery<'_>,
            _: &ApiKey,
        ) -> Result<Vec<CodeSearchResult>> {
            Ok(self.search_results.clone())
        }
        async fn list_workspaces(&self, _: &ApiKey) -> Result<Vec<WorkspaceInfo>> {
            Ok(self.workspaces.lock().await.clone())
        }
        async fn get_workspace(
            &self,
            workspace_id: &WorkspaceId,
            _: &ApiKey,
        ) -> Result<Option<WorkspaceInfo>> {
            Ok(self
                .workspaces
                .lock()
                .await
                .iter()
                .find(|w| w.workspace_id == *workspace_id)
                .cloned())
        }
        async fn list_workspace_files(
            &self,
            _: &WorkspaceFiles,
            _: &ApiKey,
        ) -> Result<Vec<FileHash>> {
            Ok(self.server_files.clone())
        }
        async fn delete_files(&self, deletion: &FileDeletion, _: &ApiKey) -> Result<()> {
            self.deleted_files
                .lock()
                .await
                .extend(deletion.data.clone());
            Ok(())
        }
        async fn delete_workspace(&self, workspace_id: &WorkspaceId, _: &ApiKey) -> Result<()> {
            self.workspaces
                .lock()
                .await
                .retain(|w| w.workspace_id != *workspace_id);
            Ok(())
        }
    }

    #[async_trait]
    impl WalkerInfra for MockInfra {
        async fn walk(&self, _: Walker) -> Result<Vec<WalkedFile>> {
            Ok(self
                .files
                .keys()
                .map(|p| WalkedFile { path: p.clone(), file_name: Some(p.clone()), size: 100 })
                .collect())
        }
    }

    #[async_trait]
    impl FileReaderInfra for MockInfra {
        async fn read_utf8(&self, path: &Path) -> Result<String> {
            self.files
                .get(path.file_name().unwrap().to_str().unwrap())
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("File not found"))
        }
        async fn read(&self, _: &Path) -> Result<Vec<u8>> {
            Ok(vec![])
        }
        async fn range_read_utf8(&self, _: &Path, _: u64, _: u64) -> Result<(String, FileInfo)> {
            Ok((
                String::new(),
                FileInfo { total_lines: 1, start_line: 1, end_line: 1 },
            ))
        }
    }

    #[tokio::test]
    async fn test_sync_new_workspace() {
        let service = ForgeIndexingService::new(Arc::new(MockInfra::new(&["main.rs", "lib.rs"])));

        let actual = service.sync_codebase(PathBuf::from("."), 20).await.unwrap();

        assert_eq!(actual.files_processed, 2);
        assert_eq!(actual.upload_stats.nodes_created, 2);
    }

    #[tokio::test]
    async fn test_query_returns_results() {
        let mut mock = MockInfra::synced(&["test.rs"]);
        mock.search_results = vec![search_result()];
        let service = ForgeIndexingService::new(Arc::new(mock));

        let params = forge_domain::SearchParams::new("test", "fest").limit(10usize);
        let actual = service
            .query_codebase(PathBuf::from("."), params)
            .await
            .unwrap();

        assert_eq!(actual.len(), 1);
    }

    #[tokio::test]
    async fn test_query_error_when_not_found() {
        let service = ForgeIndexingService::new(Arc::new(MockInfra::default()));

        let params = forge_domain::SearchParams::new("test", "fest").limit(10usize);
        let actual = service.query_codebase(PathBuf::from("."), params).await;

        assert!(actual.is_err());
        assert!(actual.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_list_codebases() {
        let ws = workspace();
        let mock = MockInfra::synced(&["test.rs"]);
        mock.workspaces.lock().await.push(WorkspaceInfo {
            workspace_id: ws.workspace_id,
            working_dir: "/project".into(),
            node_count: 0,
            relation_count: 0,
            last_updated: None,
            created_at: chrono::Utc::now(),
        });
        let service = ForgeIndexingService::new(Arc::new(mock));

        let actual = service.list_codebase().await.unwrap();

        assert_eq!(actual.len(), 1);
    }

    #[tokio::test]
    async fn test_list_codebases_error_when_none() {
        let service = ForgeIndexingService::new(Arc::new(MockInfra::default()));

        let actual = service.list_codebase().await;

        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_stale_files_deleted_and_changed_uploaded() {
        let mut mock = MockInfra::out_of_sync(
            &["changed.rs", "unchanged.rs", "new.rs"],
            &["deleted.rs", "unchanged.rs"],
        );
        mock.server_files
            .push(FileHash { path: "changed.rs".into(), hash: "old".into() });
        let service = ForgeIndexingService::new(Arc::new(mock.clone()));

        service.sync_codebase(PathBuf::from("."), 20).await.unwrap();

        let deleted = mock.deleted_files.lock().await;
        assert_eq!(deleted.len(), 2);
        assert!(deleted.contains(&"deleted.rs".into()));
        assert!(deleted.contains(&"changed.rs".into()));

        let uploaded = mock.uploaded_files.lock().await;
        assert_eq!(uploaded.len(), 2);
        assert!(uploaded.contains(&"changed.rs".into()));
        assert!(uploaded.contains(&"new.rs".into()));
    }

    #[tokio::test]
    async fn test_no_upload_when_unchanged() {
        let mock = MockInfra::synced(&["main.rs"]);
        let service = ForgeIndexingService::new(Arc::new(mock.clone()));

        let actual = service.sync_codebase(PathBuf::from("."), 20).await.unwrap();

        assert!(mock.deleted_files.lock().await.is_empty());
        assert!(mock.uploaded_files.lock().await.is_empty());
        assert_eq!(actual.upload_stats.nodes_created, 0);
    }

    #[tokio::test]
    async fn test_orphaned_files_deleted() {
        let mut mock = MockInfra::out_of_sync(&["main.rs"], &["main.rs"]);
        mock.server_files
            .push(FileHash { path: "old.rs".into(), hash: "x".into() });
        let service = ForgeIndexingService::new(Arc::new(mock.clone()));

        service.sync_codebase(PathBuf::from("."), 20).await.unwrap();

        let deleted = mock.deleted_files.lock().await;
        assert_eq!(deleted.len(), 1);
        assert!(deleted.contains(&"old.rs".into()));
        assert!(mock.uploaded_files.lock().await.is_empty());
    }

    #[tokio::test]
    async fn test_delete_codebase() {
        let ws = workspace();
        let mock = MockInfra::synced(&["main.rs"]);
        mock.workspaces.lock().await.push(WorkspaceInfo {
            workspace_id: ws.workspace_id.clone(),
            working_dir: "/project".into(),
            node_count: 0,
            relation_count: 0,
            last_updated: None,
            created_at: chrono::Utc::now(),
        });
        let service = ForgeIndexingService::new(Arc::new(mock));

        service.delete_codebase(&ws.workspace_id).await.unwrap();

        let actual = service.list_codebase().await.unwrap();
        assert!(!actual.iter().any(|w| w.workspace_id == ws.workspace_id));
    }

    #[tokio::test]
    async fn test_get_workspace_info_returns_workspace() {
        let mock = MockInfra::synced(&["main.rs"]);
        let ws = mock.workspace.clone().unwrap();
        mock.workspaces.lock().await.push(WorkspaceInfo {
            workspace_id: ws.workspace_id.clone(),
            working_dir: ws.path.to_str().unwrap().into(),
            node_count: 5,
            relation_count: 10,
            last_updated: Some(chrono::Utc::now()),
            created_at: chrono::Utc::now(),
        });
        let service = ForgeIndexingService::new(Arc::new(mock));

        let actual = service.get_workspace_info(ws.path).await.unwrap();

        assert!(actual.is_some());
        let expected = actual.unwrap();
        assert_eq!(expected.workspace_id, ws.workspace_id);
        assert_eq!(expected.node_count, 5);
        assert_eq!(expected.relation_count, 10);
    }

    #[tokio::test]
    async fn test_get_workspace_info_returns_none_when_not_found() {
        let mock = MockInfra::new(&["main.rs"]);
        let service = ForgeIndexingService::new(Arc::new(mock));

        let actual = service
            .get_workspace_info(PathBuf::from("."))
            .await
            .unwrap();

        assert!(actual.is_none());
    }

    #[tokio::test]
    async fn test_get_workspace_info_error_when_not_authenticated() {
        let mut mock = MockInfra::synced(&["main.rs"]);
        mock.authenticated = false;
        let ws = mock.workspace.clone().unwrap();
        let service = ForgeIndexingService::new(Arc::new(mock));
        let actual = service.get_workspace_info(ws.path).await;

        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("No indexing authentication found")
        );
    }
}
