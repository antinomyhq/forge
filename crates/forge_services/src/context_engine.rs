use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_app::{
    ContextEngineService, FileReaderInfra, SyncPlan, Walker, WalkerInfra, compute_hash,
};
use forge_domain::{
    AuthCredential, AuthDetails, ContextEngineRepository, FileHash, FileNode, ProviderId,
    ProviderRepository, SyncProgress, UserId, WorkspaceId, WorkspaceRepository,
};
use forge_stream::MpscStream;
use futures::future::join_all;
use tracing::{info, warn};

/// Service for indexing codebases and performing semantic search
pub struct ForgeContextEngineService<F> {
    infra: Arc<F>,
}

impl<F> Clone for ForgeContextEngineService<F> {
    fn clone(&self) -> Self {
        Self { infra: Arc::clone(&self.infra) }
    }
}

impl<F> ForgeContextEngineService<F> {
    /// Creates a new indexing service with the provided infrastructure.
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    /// Fetches remote file hashes from the server.
    async fn fetch_remote_hashes(
        &self,
        user_id: &UserId,
        workspace_id: &WorkspaceId,
        auth_token: &forge_domain::ApiKey,
    ) -> Vec<FileHash>
    where
        F: ContextEngineRepository,
    {
        info!("Fetching existing file hashes from server to detect changes...");
        let workspace_files =
            forge_domain::CodeBase::new(user_id.clone(), workspace_id.clone(), ());
        self.infra
            .list_workspace_files(&workspace_files, auth_token)
            .await
            .unwrap_or_default()
    }

    /// Deletes a batch of files from the server.
    async fn delete(
        &self,
        user_id: &UserId,
        workspace_id: &WorkspaceId,
        token: &forge_domain::ApiKey,
        paths: Vec<String>,
    ) -> Result<()>
    where
        F: ContextEngineRepository,
    {
        let deletion = forge_domain::CodeBase::new(user_id.clone(), workspace_id.clone(), paths);
        self.infra
            .delete_files(&deletion, token)
            .await
            .context("Failed to delete files")
    }

    /// Uploads a batch of files to the server.
    async fn upload(
        &self,
        user_id: &UserId,
        workspace_id: &WorkspaceId,
        token: &forge_domain::ApiKey,
        files: Vec<forge_domain::FileRead>,
    ) -> Result<()>
    where
        F: ContextEngineRepository,
    {
        let upload = forge_domain::CodeBase::new(user_id.clone(), workspace_id.clone(), files);
        self.infra
            .upload_files(&upload, token)
            .await
            .context("Failed to upload files")?;
        Ok(())
    }

    /// Internal sync implementation that emits progress events.
    async fn sync_codebase_internal<E, Fut>(
        &self,
        path: PathBuf,
        batch_size: usize,
        emit: E,
    ) -> Result<()>
    where
        F: WorkspaceRepository
            + ProviderRepository
            + ContextEngineRepository
            + WalkerInfra
            + FileReaderInfra,
        E: Fn(SyncProgress) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = ()> + Send,
    {
        info!(path = %path.display(), "Starting codebase sync");

        emit(SyncProgress::Starting).await;

        let (token, user_id) = self.get_workspace_credentials().await?;
        let path = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;
        let workspace = self.find_workspace_by_path(path.clone()).await?;

        let (workspace_id, is_new_workspace) = match workspace {
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
            // Create an workspace.
            let id = self
                .infra
                .create_workspace(&path, &token)
                .await
                .context("Failed to create workspace on server")?;

            // Save workspace in database to avoid creating multiple workspaces
            self.infra
                .upsert(&id, &user_id, &path)
                .await
                .context("Failed to save workspace")?;

            emit(SyncProgress::WorkspaceCreated { workspace_id: id.clone() }).await;
            id
        } else {
            workspace_id
        };

        // Read all files and compute hashes
        emit(SyncProgress::DiscoveringFiles { path: path.clone() }).await;
        let local_files = self.read_files(&path).await?;
        let total_file_count = local_files.len();
        emit(SyncProgress::FilesDiscovered { count: total_file_count }).await;

        let remote_files = if is_new_workspace {
            Vec::new()
        } else {
            self.fetch_remote_hashes(&user_id, &workspace_id, &token)
                .await
        };

        emit(SyncProgress::ComparingFiles {
            remote_files: remote_files.len(),
            local_files: total_file_count,
        })
        .await;

        let plan = SyncPlan::new(local_files, remote_files);
        let statuses = plan.file_statuses();

        // Compute counts from statuses
        let added = statuses
            .iter()
            .filter(|s| s.status == forge_domain::SyncStatus::New)
            .count();
        let deleted = statuses
            .iter()
            .filter(|s| s.status == forge_domain::SyncStatus::Deleted)
            .count();
        let modified = statuses
            .iter()
            .filter(|s| s.status == forge_domain::SyncStatus::Modified)
            .count();

        // Only emit diff computed event if there are actual changes
        if added + deleted + modified > 0 {
            emit(SyncProgress::DiffComputed { added, deleted, modified }).await;
        }

        let (files_to_delete, files_to_upload) = plan.get_operations();
        let total_operations = files_to_delete.len() + files_to_upload.len() - modified;

        let mut current = 0;
        emit(SyncProgress::Syncing { current: current as f64, total: total_operations }).await;

        // Delete outdated/orphaned files in batches
        for batch in files_to_delete.chunks(batch_size) {
            self.delete(&user_id, &workspace_id, &token, batch.to_vec())
                .await?;
            current += batch.len();
            emit(SyncProgress::Syncing { current: current as f64, total: total_operations }).await;
        }

        // Upload new/changed files in batches
        for batch in files_to_upload.chunks(batch_size) {
            self.upload(&user_id, &workspace_id, &token, batch.to_vec())
                .await?;
            current += batch.len();
            emit(SyncProgress::Syncing { current: current as f64, total: total_operations }).await;
        }

        // Save workspace metadata
        self.infra
            .upsert(&workspace_id, &user_id, &path)
            .await
            .context("Failed to save workspace")?;

        info!(
            workspace_id = %workspace_id,
            total_files = total_file_count,
            "Sync completed successfully"
        );

        emit(SyncProgress::Completed {
            total_files: total_file_count,
            uploaded_files: total_operations,
        })
        .await;

        Ok(())
    }

    /// Gets the forge services credential and extracts workspace auth
    /// components
    ///
    /// # Errors
    /// Returns an error if the credential is not found, if there's a database
    /// error, or if the credential format is invalid
    async fn get_workspace_credentials(&self) -> Result<(forge_domain::ApiKey, UserId)>
    where
        F: ProviderRepository,
    {
        let credential = self
            .infra
            .get_credential(&ProviderId::FORGE_SERVICES)
            .await?
            .context("No authentication credentials found. Please authenticate first.")?;

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

    /// Canonicalizes a path and finds the associated workspace
    ///
    /// # Errors
    /// Returns an error if the path cannot be canonicalized or if there's a
    /// database error. Returns Ok(None) if the workspace is not found.
    async fn find_workspace_by_path(&self, path: PathBuf) -> Result<Option<forge_domain::Workspace>>
    where
        F: WorkspaceRepository,
    {
        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

        self.infra.find_by_path(&canonical_path).await
    }

    /// Walks the directory, reads all files, and computes their hashes.
    async fn read_files(&self, dir_path: &Path) -> Result<Vec<FileNode>>
    where
        F: WalkerInfra + FileReaderInfra,
    {
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
                        FileNode { file_path: relative_path.clone(), content, hash }
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
        + FileReaderInfra
        + 'static,
> ContextEngineService for ForgeContextEngineService<F>
{
    async fn sync_codebase(
        &self,
        path: PathBuf,
        batch_size: usize,
    ) -> Result<MpscStream<Result<SyncProgress>>> {
        let service = Clone::clone(self);

        let stream = MpscStream::spawn(move |tx| async move {
            // Create emit closure that captures the sender
            let emit = |progress: SyncProgress| {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(Ok(progress)).await;
                }
            };

            // Run the sync and emit progress events
            let result = service.sync_codebase_internal(path, batch_size, emit).await;

            // If there was an error, send it through the channel
            if let Err(e) = result {
                let _ = tx.send(Err(e)).await;
            }
        });

        Ok(stream)
    }

    /// Performs semantic code search on a workspace.
    async fn query_codebase(
        &self,
        path: PathBuf,
        params: forge_domain::SearchParams<'_>,
    ) -> Result<Vec<forge_domain::Node>> {
        let (token, _) = self.get_workspace_credentials().await?;

        let workspace = self
            .find_workspace_by_path(path)
            .await?
            .ok_or(forge_domain::Error::WorkspaceNotFound)?;

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
        let (token, _) = self.get_workspace_credentials().await?;

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
        let (token, _) = self.get_workspace_credentials().await?;
        let workspace = self.find_workspace_by_path(path).await?;

        if let Some(workspace) = workspace {
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
        let (token, _) = self.get_workspace_credentials().await?;

        self.infra
            .as_ref()
            .delete_workspace(workspace_id, &token)
            .await
            .context("Failed to delete workspace from server")?;

        self.infra
            .as_ref()
            .delete(workspace_id)
            .await
            .context("Failed to delete workspace from local database")?;

        Ok(())
    }

    async fn is_indexed(&self, path: &std::path::Path) -> Result<bool> {
        match self.find_workspace_by_path(path.to_path_buf()).await {
            Ok(workspace) => Ok(workspace.is_some()),
            Err(_) => Ok(false), // Path doesn't exist or other error, so it can't be indexed
        }
    }

    async fn get_workspace_status(&self, path: PathBuf) -> Result<Vec<forge_domain::FileStatus>> {
        let (token, user_id) = self.get_workspace_credentials().await?;

        let workspace = self
            .find_workspace_by_path(path)
            .await?
            .context("Workspace not indexed. Please run `workspace sync` first.")?;

        let local_files = self.read_files(&workspace.path).await?;

        let remote_files = self
            .fetch_remote_hashes(&user_id, &workspace.workspace_id, &token)
            .await;

        let plan = SyncPlan::new(local_files, remote_files);
        Ok(plan.file_statuses())
    }

    async fn is_authenticated(&self) -> Result<bool> {
        Ok(self
            .infra
            .get_credential(&ProviderId::FORGE_SERVICES)
            .await?
            .is_some())
    }

    async fn init_auth_credentials(&self) -> Result<forge_domain::WorkspaceAuth> {
        // Authenticate with the indexing service
        let auth = self
            .infra
            .authenticate()
            .await
            .context("Failed to authenticate with indexing service")?;

        // Convert to AuthCredential and store
        let mut url_params = HashMap::new();
        url_params.insert(
            "user_id".to_string().into(),
            auth.user_id.to_string().into(),
        );

        let credential = AuthCredential {
            id: ProviderId::FORGE_SERVICES,
            auth_details: auth.clone().into(),
            url_params,
        };

        self.infra
            .upsert_credential(credential)
            .await
            .context("Failed to store authentication credentials")?;

        Ok(auth)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_app::WalkedFile;
    use forge_domain::{
        ApiKey, CodeSearchQuery, FileDeletion, FileHash, FileInfo, FileUpload, FileUploadInfo,
        Node, UserId, Workspace, WorkspaceAuth, WorkspaceFiles, WorkspaceId, WorkspaceInfo,
    };
    use futures::StreamExt;
    use pretty_assertions::assert_eq;

    use super::*;

    #[derive(Default, Clone)]
    struct MockInfra {
        files: HashMap<String, String>,
        workspace: Option<Workspace>,
        search_results: Vec<Node>,
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

    fn search_result() -> Node {
        Node {
            node_id: "n1".into(),
            node: forge_domain::NodeData::FileChunk(forge_domain::FileChunk {
                file_path: "main.rs".into(),
                content: "fn main() {}".into(),
                start_line: 1,
                end_line: 1,
            }),
            relevance: Some(0.95),
            distance: Some(0.05),
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
        async fn search(&self, _: &CodeSearchQuery<'_>, _: &ApiKey) -> Result<Vec<Node>> {
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
    async fn test_query_returns_results() {
        let mut mock = MockInfra::synced(&["test.rs"]);
        mock.search_results = vec![search_result()];
        let service = ForgeContextEngineService::new(Arc::new(mock));

        let params = forge_domain::SearchParams::new("test", "fest").limit(10usize);
        let actual = service
            .query_codebase(PathBuf::from("."), params)
            .await
            .unwrap();

        assert_eq!(actual.len(), 1);
    }

    #[tokio::test]
    async fn test_query_error_when_not_found() {
        let mut mock = MockInfra::default();
        mock.authenticated = true; // Provide authentication
        let service = ForgeContextEngineService::new(Arc::new(mock));

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
            node_count: Some(0),
            relation_count: Some(0),
            last_updated: None,
            created_at: chrono::Utc::now(),
        });
        let service = ForgeContextEngineService::new(Arc::new(mock));

        let actual = service.list_codebase().await.unwrap();

        assert_eq!(actual.len(), 1);
    }

    #[tokio::test]
    async fn test_list_codebases_error_when_none() {
        let service = ForgeContextEngineService::new(Arc::new(MockInfra::default()));

        let actual = service.list_codebase().await;

        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_orphaned_files_deleted() {
        let mut mock = MockInfra::out_of_sync(&["main.rs"], &["main.rs"]);
        mock.server_files
            .push(FileHash { path: "old.rs".into(), hash: "x".into() });
        let service = ForgeContextEngineService::new(Arc::new(mock.clone()));

        let mut stream = service.sync_codebase(PathBuf::from("."), 20).await.unwrap();

        // Consume the stream and collect events
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        // Verify events were emitted
        assert!(!events.is_empty(), "Expected progress events");

        // Verify all events succeeded
        for event in &events {
            assert!(event.is_ok(), "Expected all events to succeed");
        }

        // Verify the deleted files
        let deleted = mock.deleted_files.lock().await;
        assert_eq!(deleted.len(), 1);
        assert!(deleted.contains(&"old.rs".into()));
        assert!(mock.uploaded_files.lock().await.is_empty());
    }

    #[tokio::test]
    async fn test_sync_codebase_emits_progress_events() {
        let mock = MockInfra::out_of_sync(&["file1.rs", "file2.rs"], &[]);
        let service = ForgeContextEngineService::new(Arc::new(mock));

        let mut stream = service.sync_codebase(PathBuf::from("."), 20).await.unwrap();

        // Collect all events
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            match event {
                Ok(progress) => events.push(progress),
                Err(e) => panic!("Unexpected error: {}", e),
            }
        }

        // Verify we got progress events
        assert!(!events.is_empty(), "Expected at least one progress event");

        // Verify we got a completion event
        let has_completion = events
            .iter()
            .any(|e| matches!(e, forge_domain::SyncProgress::Completed { .. }));
        assert!(has_completion, "Expected a completion event");
    }

    #[tokio::test]
    async fn test_sync_codebase_uploads_new_files() {
        let mock = MockInfra::out_of_sync(&["new_file.rs"], &[]);
        let service = ForgeContextEngineService::new(Arc::new(mock.clone()));

        let mut stream = service.sync_codebase(PathBuf::from("."), 20).await.unwrap();

        // Consume all events
        while let Some(_event) = stream.next().await {}

        // Verify the file was uploaded
        let uploaded = mock.uploaded_files.lock().await;
        assert_eq!(uploaded.len(), 1);
        assert!(uploaded.contains(&"new_file.rs".into()));
    }

    #[tokio::test]
    async fn test_delete_codebase() {
        let ws = workspace();
        let mock = MockInfra::synced(&["main.rs"]);
        mock.workspaces.lock().await.push(WorkspaceInfo {
            workspace_id: ws.workspace_id.clone(),
            working_dir: "/project".into(),
            node_count: Some(0),
            relation_count: Some(0),
            last_updated: None,
            created_at: chrono::Utc::now(),
        });
        let service = ForgeContextEngineService::new(Arc::new(mock));

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
            node_count: Some(5),
            relation_count: Some(10),
            last_updated: Some(chrono::Utc::now()),
            created_at: chrono::Utc::now(),
        });
        let service = ForgeContextEngineService::new(Arc::new(mock));

        let actual = service.get_workspace_info(ws.path).await.unwrap();

        assert!(actual.is_some());
        let expected = actual.unwrap();
        assert_eq!(expected.workspace_id, ws.workspace_id);
        assert_eq!(expected.node_count, Some(5));
        assert_eq!(expected.relation_count, Some(10));
    }

    #[tokio::test]
    async fn test_get_workspace_info_returns_none_when_not_found() {
        let mock = MockInfra::new(&["main.rs"]);
        let service = ForgeContextEngineService::new(Arc::new(mock));

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
        let service = ForgeContextEngineService::new(Arc::new(mock));
        let actual = service.get_workspace_info(ws.path).await;

        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("No authentication credentials found")
        );
    }

    #[tokio::test]
    async fn test_get_workspace_status_all_in_sync() {
        let mock = MockInfra::synced(&["file1.rs", "file2.rs"]);
        let service = ForgeContextEngineService::new(Arc::new(mock));

        let actual = service
            .get_workspace_status(PathBuf::from("."))
            .await
            .unwrap();

        let expected = vec![
            forge_domain::FileStatus::new("file1.rs".to_string(), forge_domain::SyncStatus::InSync),
            forge_domain::FileStatus::new("file2.rs".to_string(), forge_domain::SyncStatus::InSync),
        ];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_get_workspace_status_with_modifications() {
        // Setup: local has file1.rs and file2.rs (modified), server has file1.rs and
        // file3.rs
        let mut mock = MockInfra::default();
        mock.files = [
            ("file1.rs".to_string(), "content of file1.rs".to_string()),
            ("file2.rs".to_string(), "modified content".to_string()),
        ]
        .into_iter()
        .collect();

        mock.workspace = Some(workspace());
        mock.authenticated = true;
        mock.server_files = vec![
            FileHash {
                path: "file1.rs".to_string(),
                hash: compute_hash("content of file1.rs"),
            },
            FileHash {
                path: "file2.rs".to_string(),
                hash: compute_hash("content of file2.rs"), // Different from local
            },
            FileHash {
                path: "file3.rs".to_string(),
                hash: compute_hash("content of file3.rs"),
            },
        ];

        let service = ForgeContextEngineService::new(Arc::new(mock));

        let actual = service
            .get_workspace_status(PathBuf::from("."))
            .await
            .unwrap();

        let expected = vec![
            forge_domain::FileStatus::new("file1.rs".to_string(), forge_domain::SyncStatus::InSync),
            forge_domain::FileStatus::new(
                "file2.rs".to_string(),
                forge_domain::SyncStatus::Modified,
            ),
            forge_domain::FileStatus::new(
                "file3.rs".to_string(),
                forge_domain::SyncStatus::Deleted,
            ),
        ];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_get_workspace_status_with_new_files() {
        let mut mock = MockInfra::default();
        mock.files = [
            ("file1.rs".to_string(), "content of file1.rs".to_string()),
            ("file2.rs".to_string(), "content of file2.rs".to_string()),
        ]
        .into_iter()
        .collect();

        mock.workspace = Some(workspace());
        mock.authenticated = true;
        mock.server_files = vec![FileHash {
            path: "file1.rs".to_string(),
            hash: compute_hash("content of file1.rs"),
        }];

        let service = ForgeContextEngineService::new(Arc::new(mock));

        let actual = service
            .get_workspace_status(PathBuf::from("."))
            .await
            .unwrap();

        let expected = vec![
            forge_domain::FileStatus::new("file1.rs".to_string(), forge_domain::SyncStatus::InSync),
            forge_domain::FileStatus::new("file2.rs".to_string(), forge_domain::SyncStatus::New),
        ];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_get_workspace_status_not_indexed() {
        let mut mock = MockInfra::default(); // No workspace
        mock.authenticated = true; // Provide authentication
        let service = ForgeContextEngineService::new(Arc::new(mock));

        let actual = service.get_workspace_status(PathBuf::from(".")).await;

        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("Workspace not indexed")
        );
    }
}
