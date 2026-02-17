use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use backon::{ExponentialBuilder, Retryable};
use forge_app::{
    EnvironmentInfra, FileReaderInfra, SyncProgressCounter, Walker, WalkerInfra, WorkspaceService,
    WorkspaceStatus, compute_hash,
};
use forge_domain::{
    AuthCredential, AuthDetails, Error, FileHash, FileNode, ProviderId, ProviderRepository,
    SyncProgress, UserId, WorkspaceId, WorkspaceIndexRepository, WorkspaceRepository,
};
use forge_stream::MpscStream;
use futures::future::join_all;
use futures::stream::StreamExt;
use tracing::{info, warn};

/// Loads allowed file extensions from allowed_extensions.txt into a HashSet
fn allowed_extensions() -> &'static HashSet<String> {
    static ALLOWED_EXTENSIONS: OnceLock<HashSet<String>> = OnceLock::new();
    ALLOWED_EXTENSIONS.get_or_init(|| {
        let extensions_str = include_str!("allowed_extensions.txt");
        extensions_str
            .lines()
            .map(|line| line.trim().to_lowercase())
            .filter(|line| !line.is_empty())
            .collect()
    })
}

/// Checks if a file has an allowed extension for workspace syncing (O(1)
/// lookup)
fn has_allowed_extension(path: &Path) -> bool {
    if let Some(extension) = path.extension() {
        let ext = extension.to_string_lossy().to_lowercase();
        allowed_extensions().contains(&ext)
    } else {
        false
    }
}

/// Determines if a gRPC error should trigger a retry attempt.
///
/// Only retries transient errors that might succeed on subsequent attempts.
/// Permanent failures (client errors, unimplemented operations, auth issues)
/// are not retried.
fn should_retry_grpc(error: &anyhow::Error) -> bool {
    // Check if this is a connection error (service offline)
    // Connection errors indicate the service is not running and retrying won't help
    let error_msg = format!("{:#}", error).to_lowercase();
    if error_msg.contains("connection refused") || error_msg.contains("tcp connect error") {
        return false;
    }

    warn!(error=?error,"Attempting to retry");
    let Some(status) = error.downcast_ref::<tonic::Status>() else {
        // Not a gRPC error, retry by default
        return true;
    };

    // Only retry transient errors that might succeed on subsequent attempts
    matches!(
        status.code(),
        tonic::Code::Cancelled           // Operation cancelled, may succeed if retried
        | tonic::Code::Unknown           // Unknown error, might be transient
        | tonic::Code::DeadlineExceeded  // Timeout, may succeed with more time
        | tonic::Code::ResourceExhausted // Rate limiting or quota, may recover
        | tonic::Code::Aborted           // Conflict or transaction abort, may succeed
        | tonic::Code::Unavailable // Service unavailable, classic retry case
    )
}

/// Service for indexing workspaces and performing semantic search
pub struct ForgeWorkspaceService<F> {
    infra: Arc<F>,
}

impl<F> Clone for ForgeWorkspaceService<F> {
    fn clone(&self) -> Self {
        Self { infra: Arc::clone(&self.infra) }
    }
}

impl<F> ForgeWorkspaceService<F> {
    /// Creates a new indexing service with the provided infrastructure.
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    /// Execute an operation with retry logic based on the retry configuration
    async fn with_retry<Fut, T>(&self, operation: impl Fn() -> Fut) -> Result<T>
    where
        F: EnvironmentInfra,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let env = self.infra.get_environment();
        let retry_config = &env.retry_config;

        let mut builder = ExponentialBuilder::default()
            .with_min_delay(Duration::from_millis(retry_config.min_delay_ms))
            .with_factor(retry_config.backoff_factor as f32)
            .with_max_times(retry_config.max_retry_attempts)
            .with_jitter();

        if let Some(max_delay) = retry_config.max_delay {
            builder = builder.with_max_delay(Duration::from_secs(max_delay));
        }

        operation.retry(builder).when(should_retry_grpc).await
    }

    /// Fetches remote file hashes from the server.
    async fn fetch_remote_hashes(
        &self,
        user_id: &UserId,
        workspace_id: &WorkspaceId,
        auth_token: &forge_domain::ApiKey,
    ) -> Vec<FileHash>
    where
        F: WorkspaceIndexRepository + EnvironmentInfra,
    {
        info!("Fetching existing file hashes from server to detect changes...");
        let workspace_files =
            forge_domain::CodeBase::new(user_id.clone(), workspace_id.clone(), ());

        self.with_retry(|| {
            self.infra
                .list_workspace_files(&workspace_files, auth_token)
        })
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
        F: WorkspaceIndexRepository + EnvironmentInfra,
    {
        let deletion = forge_domain::CodeBase::new(user_id.clone(), workspace_id.clone(), paths);

        self.with_retry(|| self.infra.delete_files(&deletion, token))
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
        F: WorkspaceIndexRepository + EnvironmentInfra,
    {
        let upload = forge_domain::CodeBase::new(user_id.clone(), workspace_id.clone(), files);

        self.with_retry(|| self.infra.upload_files(&upload, token))
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
            + WorkspaceIndexRepository
            + WalkerInfra
            + FileReaderInfra
            + EnvironmentInfra,
        E: Fn(SyncProgress) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = ()> + Send,
    {
        info!(path = %path.display(), "Starting workspace sync");

        emit(SyncProgress::Starting).await;

        let (token, user_id) = self.get_workspace_credentials().await?;
        let path = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

        // Find workspace by exact match or ancestor
        let workspace = self.find_workspace_by_path(path.clone(), &user_id).await?;

        let (workspace_id, workspace_path, is_new_workspace) = match workspace {
            Some(workspace) if workspace.user_id == user_id => {
                (workspace.workspace_id, workspace.path, false)
            }
            Some(workspace) => {
                // Found workspace but different user - delete and create new
                if let Err(e) = self.infra.delete(&workspace.workspace_id).await {
                    warn!(error = %e, "Failed to delete old workspace entry from local database");
                }
                (WorkspaceId::generate(), path.clone(), true)
            }
            None => {
                // No workspace found - create new
                (WorkspaceId::generate(), path.clone(), true)
            }
        };

        let workspace_id = if is_new_workspace {
            // Create an workspace.
            let id = self
                .with_retry(|| self.infra.create_workspace(&workspace_path, &token))
                .await
                .context("Failed to create workspace on server")?;

            // Save workspace in database to avoid creating multiple workspaces
            self.infra
                .upsert(&id, &user_id, &workspace_path)
                .await
                .context("Failed to save workspace")?;

            emit(SyncProgress::WorkspaceCreated { workspace_id: id.clone() }).await;
            id
        } else {
            workspace_id
        };

        // Read all files and compute hashes from the workspace root path
        emit(SyncProgress::DiscoveringFiles { path: workspace_path.clone() }).await;
        let local_files = self.read_files(&workspace_path).await?;
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

        let plan = WorkspaceStatus::new(local_files, remote_files);
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

        // Compute total number of affected files
        let total_file_changes = added + deleted + modified;

        // Only emit diff computed event if there are actual changes
        if total_file_changes > 0 {
            emit(SyncProgress::DiffComputed { added, deleted, modified }).await;
        }

        let (files_to_delete, files_to_upload) = plan.get_operations();

        let total_operations = files_to_delete.len() + files_to_upload.len();
        let mut counter = SyncProgressCounter::new(total_file_changes, total_operations);
        let mut failed_files = 0;

        emit(counter.sync_progress()).await;

        let mut delete_stream = futures::stream::iter(files_to_delete)
            .map(|path| {
                let user_id = user_id.clone();
                let workspace_id = workspace_id.clone();
                let token = token.clone();
                async move {
                    self.delete(&user_id, &workspace_id, &token, vec![path])
                        .await?;
                    Ok::<_, anyhow::Error>(1)
                }
            })
            .buffer_unordered(batch_size);

        // Process deletions as they complete, updating progress incrementally
        while let Some(result) = delete_stream.next().await {
            match result {
                Ok(count) => {
                    counter.complete(count);
                    emit(counter.sync_progress()).await;
                }
                Err(e) => {
                    warn!("Failed to delete file during sync: {:#}", e);
                    failed_files += 1;
                    // Continue processing remaining deletions
                }
            }
        }

        // Upload new/changed files with concurrency limit
        let mut upload_stream = futures::stream::iter(files_to_upload)
            .map(|file| {
                let user_id = user_id.clone();
                let workspace_id = workspace_id.clone();
                let token = token.clone();
                async move {
                    self.upload(&user_id, &workspace_id, &token, vec![file])
                        .await?;
                    Ok::<_, anyhow::Error>(1)
                }
            })
            .buffer_unordered(batch_size);

        // Process uploads as they complete, updating progress incrementally
        while let Some(result) = upload_stream.next().await {
            match result {
                Ok(count) => {
                    counter.complete(count);
                    emit(counter.sync_progress()).await;
                }
                Err(e) => {
                    warn!("Failed to upload file during sync: {:#}", e);
                    failed_files += 1;
                    // Continue processing remaining uploads
                }
            }
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
            uploaded_files: total_file_changes,
            failed_files,
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

    /// Finds a workspace by exact path match, or falls back to ancestor lookup
    ///
    /// Business logic:
    /// 1. First tries to find an exact match for the given path
    /// 2. If not found, searches for ancestor workspaces
    /// 3. Returns the closest ancestor (longest matching path prefix)
    ///
    /// # Errors
    /// Returns an error if the path cannot be canonicalized or if there's a
    /// database error. Returns Ok(None) if no workspace is found.
    async fn find_workspace_by_path(
        &self,
        path: PathBuf,
        user_id: &forge_domain::UserId,
    ) -> Result<Option<forge_domain::Workspace>>
    where
        F: WorkspaceRepository,
    {
        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

        // Get all workspaces for the user - let the service handle filtering
        let workspaces = self.infra.list().await?;

        // Business logic: choose which workspace to use
        // 1. First check for exact match
        if let Some(exact_match) = workspaces
            .iter()
            .find(|w| w.path == canonical_path && w.user_id == *user_id)
        {
            return Ok(Some(exact_match.clone()));
        }

        // 2. Find closest ancestor (longest matching path prefix)
        let mut best_match: Option<(&forge_domain::Workspace, usize)> = None;

        for workspace in &workspaces {
            if canonical_path.starts_with(&workspace.path) {
                let path_len = workspace.path.as_os_str().len();
                if best_match.is_none_or(|(_, len)| path_len > len) {
                    best_match = Some((workspace, path_len));
                }
            }
        }

        Ok(best_match.map(|(w, _)| w.clone()))
    }
    /// Walks the directory, reads all files, and computes their hashes.
    /// Only includes files with allowed extensions.
    async fn read_files(&self, dir_path: &Path) -> Result<Vec<FileNode>>
    where
        F: WalkerInfra + FileReaderInfra,
    {
        info!("Walking directory to discover files");
        let walker_config = Walker::unlimited()
            .cwd(dir_path.to_path_buf())
            .skip_binary(true); // Walker filters binary files

        let walked_files = self
            .infra
            .walk(walker_config)
            .await
            .context("Failed to walk directory")?
            .into_iter()
            .filter(|f| !f.is_dir())
            .collect::<Vec<_>>();

        info!(
            file_count = walked_files.len(),
            "Discovered files from walker"
        );

        // Filter files by allowed extension (pure function, no I/O)
        let filtered_files: Vec<_> = walked_files
            .into_iter()
            .filter(|walked| {
                let file_path = dir_path.join(&walked.path);
                has_allowed_extension(&file_path)
            })
            .collect();

        info!(
            filtered_count = filtered_files.len(),
            "Files after extension filtering"
        );
        anyhow::ensure!(
            !filtered_files.is_empty(),
            "No valid source files found to index"
        );

        // Read all filtered files
        let infra = self.infra.clone();
        let read_tasks = filtered_files.into_iter().map(|walked| {
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
        + WorkspaceIndexRepository
        + WalkerInfra
        + FileReaderInfra
        + EnvironmentInfra
        + 'static,
> WorkspaceService for ForgeWorkspaceService<F>
{
    async fn sync_workspace(
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
    async fn query_workspace(
        &self,
        path: PathBuf,
        params: forge_domain::SearchParams<'_>,
    ) -> Result<Vec<forge_domain::Node>> {
        let (token, user_id) = self.get_workspace_credentials().await?;

        let workspace = self
            .find_workspace_by_path(path, &user_id)
            .await?
            .ok_or(forge_domain::Error::WorkspaceNotFound)?;

        let search_query = forge_domain::CodeBase::new(
            workspace.user_id.clone(),
            workspace.workspace_id.clone(),
            params,
        );

        let results = self
            .with_retry(|| self.infra.search(&search_query, &token))
            .await
            .context("Failed to search")?;

        Ok(results)
    }

    /// Lists all workspaces.
    async fn list_workspaces(&self) -> Result<Vec<forge_domain::WorkspaceInfo>> {
        let (token, _) = self.get_workspace_credentials().await?;

        self.with_retry(|| self.infra.as_ref().list_workspaces(&token))
            .await
            .context("Failed to list workspaces")
    }

    /// Retrieves workspace information for a specific path.
    async fn get_workspace_info(&self, path: PathBuf) -> Result<Option<forge_domain::WorkspaceInfo>>
    where
        F: WorkspaceRepository + WorkspaceIndexRepository + ProviderRepository,
    {
        let (token, user_id) = self.get_workspace_credentials().await?;
        let workspace = self.find_workspace_by_path(path, &user_id).await?;

        if let Some(workspace) = workspace {
            self.with_retry(|| {
                self.infra
                    .as_ref()
                    .get_workspace(&workspace.workspace_id, &token)
            })
            .await
            .context("Failed to get workspace info")
        } else {
            Ok(None)
        }
    }

    /// Deletes a workspace from both the server and local database.
    async fn delete_workspace(&self, workspace_id: &forge_domain::WorkspaceId) -> Result<()> {
        let (token, _) = self.get_workspace_credentials().await?;

        self.with_retry(|| self.infra.as_ref().delete_workspace(workspace_id, &token))
            .await
            .context("Failed to delete workspace from server")?;

        self.infra
            .as_ref()
            .delete(workspace_id)
            .await
            .context("Failed to delete workspace from local database")?;

        Ok(())
    }

    /// Deletes multiple workspaces in parallel from both the server and local
    /// database.
    async fn delete_workspaces(&self, workspace_ids: &[forge_domain::WorkspaceId]) -> Result<()> {
        // Delete all workspaces in parallel by calling delete_workspace for each
        let delete_tasks: Vec<_> = workspace_ids
            .iter()
            .map(|workspace_id| self.delete_workspace(workspace_id))
            .collect();

        let results = join_all(delete_tasks).await;

        // Collect all errors
        let errors: Vec<_> = results.into_iter().filter_map(|r| r.err()).collect();

        if !errors.is_empty() {
            return Err(anyhow::anyhow!(
                "Failed to delete {} workspace(s): [{}]",
                errors.len(),
                errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        Ok(())
    }

    async fn is_indexed(&self, path: &std::path::Path) -> Result<bool> {
        let (_, user_id) = self.get_workspace_credentials().await?;
        match self
            .find_workspace_by_path(path.to_path_buf(), &user_id)
            .await
        {
            Ok(workspace) => Ok(workspace.is_some()),
            Err(_) => Ok(false), // Path doesn't exist or other error, so it can't be indexed
        }
    }

    async fn get_workspace_status(&self, path: PathBuf) -> Result<Vec<forge_domain::FileStatus>> {
        let (token, user_id) = self.get_workspace_credentials().await?;

        let workspace = self
            .find_workspace_by_path(path, &user_id)
            .await?
            .context("Workspace not indexed. Please run `workspace sync` first.")?;

        let local_files = self.read_files(&workspace.path).await?;

        let remote_files = self
            .fetch_remote_hashes(&user_id, &workspace.workspace_id, &token)
            .await;

        let plan = WorkspaceStatus::new(local_files, remote_files);
        Ok(plan.file_statuses())
    }

    async fn is_authenticated(&self) -> Result<bool> {
        Ok(self
            .infra
            .get_credential(&ProviderId::FORGE_SERVICES)
            .await?
            .is_some())
    }

    async fn init_auth_credentials(&self) -> Result<forge_domain::WorkspaceAuth>
    where
        F: EnvironmentInfra,
    {
        // Authenticate with the indexing service
        let auth = self
            .with_retry(|| self.infra.authenticate())
            .await
            .context("Failed to authenticate with indexing service")?;

        // Get workspace server URL from environment
        let env = self.infra.get_environment();
        let workspace_server_url = env.workspace_server_url.to_string();

        // Convert to AuthCredential and store
        let mut url_params = HashMap::new();
        url_params.insert(
            "user_id".to_string().into(),
            auth.user_id.to_string().into(),
        );
        url_params.insert(
            "FORGE_WORKSPACE_SERVER_URL".to_string().into(),
            workspace_server_url.into(),
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

    async fn init_workspace(&self, path: PathBuf) -> Result<WorkspaceId> {
        let (token, user_id) = self.get_workspace_credentials().await?;
        let path = path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

        // Find workspace by exact match or ancestor
        let workspace = self.find_workspace_by_path(path.clone(), &user_id).await?;

        // Check if workspace already exists
        if let Some(workspace) = workspace {
            if workspace.user_id == user_id {
                // Workspace already exists for this user
                return Err(Error::WorkspaceAlreadyInitialized(workspace.workspace_id).into());
            } else {
                // Found workspace but different user - delete old one
                if let Err(e) = self.infra.delete(&workspace.workspace_id).await {
                    warn!(error = %e, "Failed to delete old workspace entry from local database");
                }
            }
        }

        // Create new workspace on server
        let workspace_id = self
            .with_retry(|| self.infra.create_workspace(&path, &token))
            .await
            .context("Failed to create workspace on server")?;

        // Save workspace in database
        self.infra
            .upsert(&workspace_id, &user_id, &path)
            .await
            .context("Failed to save workspace")?;

        Ok(workspace_id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_app::{WalkedFile, WorkspaceService};
    use forge_domain::{
        ApiKey, ChatRepository, CodeSearchQuery, FileDeletion, FileHash, FileInfo, FileUpload,
        FileUploadInfo, Node, ProviderTemplate, UserId, Workspace, WorkspaceAuth, WorkspaceFiles,
        WorkspaceId, WorkspaceInfo,
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
        ancestor_workspace: Option<Workspace>, // For testing ancestor lookup
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
        // Use canonicalized current directory for tests
        let current_dir = std::env::current_dir().unwrap();
        Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: UserId::generate(),
            path: current_dir,
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
    impl ChatRepository for MockInfra {
        async fn chat(
            &self,
            _model_id: &forge_app::domain::ModelId,
            _context: forge_app::domain::Context,
            _provider: forge_domain::Provider<url::Url>,
        ) -> forge_app::domain::ResultStream<forge_app::domain::ChatCompletionMessage, anyhow::Error>
        {
            Ok(Box::pin(tokio_stream::iter(vec![])))
        }

        async fn models(
            &self,
            _provider: forge_domain::Provider<url::Url>,
        ) -> Result<Vec<forge_app::domain::Model>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockInfra {
        async fn get_all_providers(&self) -> Result<Vec<forge_domain::AnyProvider>> {
            Ok(vec![])
        }

        async fn get_provider(&self, _id: ProviderId) -> Result<ProviderTemplate> {
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
                    .or(self.ancestor_workspace.as_ref())
                    .map(|w| w.user_id.clone())
                    .unwrap_or_else(UserId::generate);

                let mut url_params = std::collections::HashMap::new();
                url_params.insert("user_id".to_string().into(), user_id.to_string().into());
                url_params.insert(
                    "FORGE_WORKSPACE_SERVER_URL".to_string().into(),
                    "https://api.forgecode.dev/".to_string().into(),
                );

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
        async fn list(&self) -> Result<Vec<Workspace>> {
            let mut workspaces = Vec::new();

            // Return all workspaces for the user (repository doesn't filter by path)
            if let Some(workspace) = &self.workspace {
                workspaces.push(workspace.clone());
            }

            if let Some(ancestor) = &self.ancestor_workspace {
                workspaces.push(ancestor.clone());
            }

            Ok(workspaces)
        }
        async fn get_user_id(&self) -> Result<Option<UserId>> {
            // Return user_id from either workspace or ancestor_workspace
            Ok(self
                .workspace
                .as_ref()
                .or(self.ancestor_workspace.as_ref())
                .map(|w| w.user_id.clone()))
        }
        async fn delete(&self, _: &WorkspaceId) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl WorkspaceIndexRepository for MockInfra {
        async fn authenticate(&self) -> Result<WorkspaceAuth> {
            // Mock authentication - return user_id from workspace or ancestor_workspace
            let user_id = self
                .workspace
                .as_ref()
                .or(self.ancestor_workspace.as_ref())
                .map(|w| w.user_id.clone())
                .unwrap_or_else(UserId::generate);

            Ok(WorkspaceAuth::new(user_id, "test_token".to_string().into()))
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

    impl forge_app::EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> forge_domain::Environment {
            use fake::{Fake, Faker};
            Faker.fake()
        }

        fn get_env_var(&self, _: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> std::collections::BTreeMap<String, String> {
            std::collections::BTreeMap::new()
        }

        fn is_restricted(&self) -> bool {
            false
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
        let service = ForgeWorkspaceService::new(Arc::new(mock));

        let params = forge_domain::SearchParams::new("test", "fest").limit(10usize);
        let actual = service
            .query_workspace(PathBuf::from("."), params)
            .await
            .unwrap();

        assert_eq!(actual.len(), 1);
    }

    #[tokio::test]
    async fn test_query_error_when_not_found() {
        let mock = MockInfra { authenticated: true, ..Default::default() };
        let service = ForgeWorkspaceService::new(Arc::new(mock));

        let params = forge_domain::SearchParams::new("test", "fest").limit(10usize);
        let actual = service.query_workspace(PathBuf::from("."), params).await;

        assert!(actual.is_err());
        assert!(actual.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_sync_filters_non_source_files() {
        // Setup with various file types - source and text files should be synced,
        // binaries filtered
        let mut files = HashMap::new();
        files.insert("source.rs".to_string(), "fn main() {}".to_string());
        files.insert("image.png".to_string(), "binary content".to_string());
        files.insert("document.pdf".to_string(), "pdf content".to_string());
        files.insert("readme.md".to_string(), "# Readme".to_string());

        let mock = MockInfra {
            files,
            workspace: None,
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock.clone()));

        let mut stream = service
            .sync_workspace(PathBuf::from("."), 20)
            .await
            .unwrap();

        // Consume the stream
        while stream.next().await.is_some() {}

        // source.rs and readme.md should be uploaded (both have allowed extensions)
        // image.png and document.pdf should be filtered (not in allowed extensions)
        let uploaded = mock.uploaded_files.lock().await;
        assert_eq!(uploaded.len(), 2);
        assert!(uploaded.contains(&"source.rs".into()));
        assert!(uploaded.contains(&"readme.md".into()));
        assert!(!uploaded.contains(&"image.png".into()));
        assert!(!uploaded.contains(&"document.pdf".into()));
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
        let service = ForgeWorkspaceService::new(Arc::new(mock));

        let actual = service.list_workspaces().await.unwrap();

        assert_eq!(actual.len(), 1);
    }

    #[tokio::test]
    async fn test_list_codebases_error_when_none() {
        let service = ForgeWorkspaceService::new(Arc::new(MockInfra::default()));

        let actual = service.list_workspaces().await;

        assert!(actual.is_err());
    }

    #[tokio::test]
    async fn test_orphaned_files_deleted() {
        let mut mock = MockInfra::out_of_sync(&["main.rs"], &["main.rs"]);
        mock.server_files
            .push(FileHash { path: "old.rs".into(), hash: "x".into() });
        let service = ForgeWorkspaceService::new(Arc::new(mock.clone()));

        let mut stream = service
            .sync_workspace(PathBuf::from("."), 20)
            .await
            .unwrap();

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
        let service = ForgeWorkspaceService::new(Arc::new(mock));

        let mut stream = service
            .sync_workspace(PathBuf::from("."), 20)
            .await
            .unwrap();

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
    async fn test_delete_multiple_workspaces() {
        let ws1 = workspace();
        let ws2 = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: ws1.user_id.clone(),
            path: PathBuf::from("/project2"),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra::synced(&["main.rs"]);
        mock.workspaces.lock().await.push(WorkspaceInfo {
            workspace_id: ws1.workspace_id.clone(),
            working_dir: "/project".into(),
            node_count: Some(0),
            relation_count: Some(0),
            last_updated: None,
            created_at: chrono::Utc::now(),
        });
        mock.workspaces.lock().await.push(WorkspaceInfo {
            workspace_id: ws2.workspace_id.clone(),
            working_dir: "/project2".into(),
            node_count: Some(0),
            relation_count: Some(0),
            last_updated: None,
            created_at: chrono::Utc::now(),
        });

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        // Delete both workspaces
        service
            .delete_workspaces(&[ws1.workspace_id.clone(), ws2.workspace_id.clone()])
            .await
            .unwrap();

        // Verify both workspaces are deleted
        let actual = service.list_workspaces().await.unwrap();
        assert!(!actual.iter().any(|w| w.workspace_id == ws1.workspace_id));
        assert!(!actual.iter().any(|w| w.workspace_id == ws2.workspace_id));
    }

    #[tokio::test]
    async fn test_sync_codebase_uploads_new_files() {
        let mock = MockInfra::out_of_sync(&["new_file.rs"], &[]);
        let service = ForgeWorkspaceService::new(Arc::new(mock.clone()));

        let mut stream = service
            .sync_workspace(PathBuf::from("."), 20)
            .await
            .unwrap();

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
        let service = ForgeWorkspaceService::new(Arc::new(mock));

        service.delete_workspace(&ws.workspace_id).await.unwrap();

        let actual = service.list_workspaces().await.unwrap();
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
        let service = ForgeWorkspaceService::new(Arc::new(mock));

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
        let service = ForgeWorkspaceService::new(Arc::new(mock));

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
        let service = ForgeWorkspaceService::new(Arc::new(mock));
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
        let service = ForgeWorkspaceService::new(Arc::new(mock));

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
        let mock = MockInfra {
            files: [
                ("file1.rs".to_string(), "content of file1.rs".to_string()),
                ("file2.rs".to_string(), "modified content".to_string()),
            ]
            .into_iter()
            .collect(),
            workspace: Some(workspace()),
            authenticated: true,
            server_files: vec![
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
            ],
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

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
        let mock = MockInfra {
            files: [
                ("file1.rs".to_string(), "content of file1.rs".to_string()),
                ("file2.rs".to_string(), "content of file2.rs".to_string()),
            ]
            .into_iter()
            .collect(),
            workspace: Some(workspace()),
            authenticated: true,
            server_files: vec![FileHash {
                path: "file1.rs".to_string(),
                hash: compute_hash("content of file1.rs"),
            }],
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

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
        let mock = MockInfra { authenticated: true, ..Default::default() };
        let service = ForgeWorkspaceService::new(Arc::new(mock));

        let actual = service.get_workspace_status(PathBuf::from(".")).await;

        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("Workspace not indexed")
        );
    }

    #[tokio::test]
    async fn test_sync_reuses_ancestor_workspace_for_subdirectory() {
        // Use current directory as parent workspace - must be canonicalized
        let current_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

        let parent_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: UserId::generate(),
            path: current_dir.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            files: [("src/main.rs", "fn main() {}")]
                .iter()
                .map(|(p, c)| (p.to_string(), c.to_string()))
                .collect(),
            workspace: Some(parent_workspace.clone()), // Exact match for current directory
            ancestor_workspace: None,
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock.clone()));

        // Sync from current directory (which should match the parent workspace exactly)
        let mut stream = service.sync_workspace(current_dir, 20).await.unwrap();

        // Collect all events
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        // Verify workspace was reused (no WorkspaceCreated event)
        let has_workspace_created = events
            .iter()
            .any(|e| matches!(e, forge_domain::SyncProgress::WorkspaceCreated { .. }));
        assert!(
            !has_workspace_created,
            "Expected no WorkspaceCreated event when reusing ancestor"
        );

        // Verify completion event exists
        let completion_event = events
            .iter()
            .find(|e| matches!(e, forge_domain::SyncProgress::Completed { .. }));
        assert!(completion_event.is_some(), "Expected completion event");
    }

    #[tokio::test]
    async fn test_sync_creates_new_workspace_when_no_ancestor() {
        let mock = MockInfra {
            files: [("src/main.rs", "fn main() {}")]
                .iter()
                .map(|(p, c)| (p.to_string(), c.to_string()))
                .collect(),
            workspace: None,          // No exact match
            ancestor_workspace: None, // No ancestor either
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock.clone()));

        let mut stream = service
            .sync_workspace(PathBuf::from("."), 20)
            .await
            .unwrap();

        // Collect all events
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        // Verify workspace was created
        let has_workspace_created = events
            .iter()
            .any(|e| matches!(e, forge_domain::SyncProgress::WorkspaceCreated { .. }));
        assert!(
            has_workspace_created,
            "Expected WorkspaceCreated event when no ancestor exists"
        );
    }

    #[tokio::test]
    async fn test_init_workspace_creates_new_workspace() {
        let mock = MockInfra {
            files: [("src/main.rs", "fn main() {}")]
                .iter()
                .map(|(p, c)| (p.to_string(), c.to_string()))
                .collect(),
            workspace: None, // No workspace yet
            ancestor_workspace: None,
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock.clone()));

        // Initialize workspace without syncing
        let workspace_id = service
            .init_workspace(std::env::current_dir().unwrap())
            .await
            .unwrap();

        // Verify we got a valid workspace ID
        assert!(!workspace_id.to_string().is_empty());
    }

    #[tokio::test]
    async fn test_init_workspace_reuses_existing_workspace() {
        let current_dir = std::env::current_dir().unwrap();
        let user_id = UserId::generate();
        let existing_workspace_id = WorkspaceId::generate();

        let existing_workspace = Workspace {
            workspace_id: existing_workspace_id.clone(),
            user_id: user_id.clone(),
            path: current_dir.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            files: [("src/main.rs", "fn main() {}")]
                .iter()
                .map(|(p, c)| (p.to_string(), c.to_string()))
                .collect(),
            workspace: Some(existing_workspace),
            ancestor_workspace: None,
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock.clone()));

        // Initialize workspace - should reuse existing
        let result = service.init_workspace(current_dir).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let domain_error = err.downcast_ref::<Error>();
        assert!(matches!(
            domain_error,
            Some(Error::WorkspaceAlreadyInitialized(id)) if id == &existing_workspace_id
        ));
    }

    #[tokio::test]
    async fn test_sync_prefers_exact_match_over_ancestor() {
        // Use current directory as exact match
        let current_dir = std::env::current_dir().unwrap();
        let user_id = UserId::generate();

        let exact_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: current_dir.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let ancestor_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: current_dir.parent().unwrap().to_path_buf(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            files: [("main.rs", "fn main() {}")]
                .iter()
                .map(|(p, c)| (p.to_string(), c.to_string()))
                .collect(),
            workspace: Some(exact_workspace.clone()), // Exact match exists
            ancestor_workspace: Some(ancestor_workspace), // Ancestor also exists
            authenticated: true,
            server_files: vec![],
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock.clone()));

        let mut stream = service.sync_workspace(current_dir, 20).await.unwrap();

        // Collect all events
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        // Verify no new workspace was created (reused exact match)
        let has_workspace_created = events
            .iter()
            .any(|e| matches!(e, forge_domain::SyncProgress::WorkspaceCreated { .. }));
        assert!(
            !has_workspace_created,
            "Expected no WorkspaceCreated event when exact match exists"
        );
    }

    #[tokio::test]
    async fn test_is_indexed_returns_true_for_ancestor_workspace() {
        // Create temporary directories for testing
        let temp_dir = std::env::temp_dir().join(format!(
            "forge_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let subdirectory = temp_dir.join("subdirectory");
        std::fs::create_dir_all(&subdirectory).unwrap();

        // Canonicalize paths for consistency
        let temp_dir_canonical = temp_dir.canonicalize().unwrap();
        let subdirectory_canonical = subdirectory.canonicalize().unwrap();

        let parent_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: UserId::generate(),
            path: temp_dir_canonical.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            workspace: None,                            // No exact match
            ancestor_workspace: Some(parent_workspace), // Parent is indexed
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        // Check the subdirectory - should find ancestor
        let actual = service.is_indexed(&subdirectory_canonical).await.unwrap();

        assert!(
            actual,
            "Expected subdirectory to be considered indexed when parent is indexed"
        );

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[tokio::test]
    async fn test_get_workspace_info_finds_ancestor() {
        // Create temporary directories for testing
        let temp_dir = std::env::temp_dir().join(format!(
            "forge_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let subdirectory = temp_dir.join("subdirectory");
        std::fs::create_dir_all(&subdirectory).unwrap();

        // Canonicalize paths for consistency
        let temp_dir_canonical = temp_dir.canonicalize().unwrap();
        let subdirectory_canonical = subdirectory.canonicalize().unwrap();

        let parent_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: UserId::generate(),
            path: temp_dir_canonical.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let workspace_info = WorkspaceInfo {
            workspace_id: parent_workspace.workspace_id.clone(),
            working_dir: parent_workspace.path.to_str().unwrap().into(),
            node_count: Some(10),
            relation_count: Some(5),
            last_updated: Some(chrono::Utc::now()),
            created_at: chrono::Utc::now(),
        };

        let mock = MockInfra {
            workspace: None,                            // No exact match
            ancestor_workspace: Some(parent_workspace), // Parent is indexed
            workspaces: Arc::new(tokio::sync::Mutex::new(vec![workspace_info.clone()])),
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        // Check a subdirectory - should find ancestor
        let actual = service
            .get_workspace_info(subdirectory_canonical)
            .await
            .unwrap();

        assert!(
            actual.is_some(),
            "Expected to find workspace info for subdirectory via ancestor"
        );
        assert_eq!(actual.unwrap().workspace_id, workspace_info.workspace_id);

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[tokio::test]
    async fn test_get_workspace_status_uses_ancestor() {
        // Create temporary directories for testing
        let temp_dir = std::env::temp_dir().join(format!(
            "forge_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let subdirectory = temp_dir.join("subdirectory");
        std::fs::create_dir_all(&subdirectory).unwrap();

        // Canonicalize paths for consistency
        let temp_dir_canonical = temp_dir.canonicalize().unwrap();
        let subdirectory_canonical = subdirectory.canonicalize().unwrap();

        let parent_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: UserId::generate(),
            path: temp_dir_canonical.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            files: [("src/main.rs", "fn main() {}")]
                .iter()
                .map(|(p, c)| (p.to_string(), c.to_string()))
                .collect(),
            workspace: None,                            // No exact match
            ancestor_workspace: Some(parent_workspace), // Parent is indexed
            authenticated: true,
            server_files: vec![],
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        // Check a subdirectory - should find ancestor
        let actual = service.get_workspace_status(subdirectory_canonical).await;

        assert!(
            actual.is_ok(),
            "Expected workspace status to work with ancestor workspace"
        );

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    // Tests for find_workspace_by_path business logic (moved from repository layer)

    #[tokio::test]
    async fn test_find_workspace_by_path_returns_exact_match() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace_path = temp_dir.path().canonicalize().unwrap();
        let user_id = UserId::generate();

        let workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: workspace_path.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            workspace: Some(workspace.clone()),
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        let actual = service
            .find_workspace_by_path(workspace_path.clone(), &user_id)
            .await
            .unwrap();

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().workspace_id, workspace.workspace_id);
    }

    #[tokio::test]
    async fn test_find_workspace_by_path_returns_ancestor_for_subdirectory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let parent_path = temp_dir.path().canonicalize().unwrap();
        let user_id = UserId::generate();

        // Create actual subdirectory
        let child_dir = parent_path.join("src");
        std::fs::create_dir(&child_dir).unwrap();
        let child_path = child_dir.canonicalize().unwrap();

        let parent_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: parent_path.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            workspace: Some(parent_workspace.clone()),
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        // Query from child directory - should find parent
        let actual = service
            .find_workspace_by_path(child_path, &user_id)
            .await
            .unwrap();

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().workspace_id, parent_workspace.workspace_id);
    }

    #[tokio::test]
    async fn test_find_workspace_by_path_deep_nesting_finds_ancestor() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path().canonicalize().unwrap();
        let user_id = UserId::generate();

        // Create deep nested directory
        let deep_child = root_path.join("src").join("components").join("ui");
        std::fs::create_dir_all(&deep_child).unwrap();
        let deep_child_path = deep_child.canonicalize().unwrap();

        let root_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: root_path.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            workspace: Some(root_workspace.clone()),
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        // Query from deeply nested directory - should find root ancestor
        let actual = service
            .find_workspace_by_path(deep_child_path, &user_id)
            .await
            .unwrap();

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().workspace_id, root_workspace.workspace_id);
    }

    #[tokio::test]
    async fn test_find_workspace_by_path_prefers_exact_match_over_ancestor() {
        let temp_dir = tempfile::tempdir().unwrap();
        let parent_path = temp_dir.path().canonicalize().unwrap();
        let user_id = UserId::generate();

        // Create subdirectory
        let child_dir = parent_path.join("src");
        std::fs::create_dir(&child_dir).unwrap();
        let child_path = child_dir.canonicalize().unwrap();

        let parent_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: parent_path.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let child_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: child_path.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            workspace: Some(child_workspace.clone()),
            ancestor_workspace: Some(parent_workspace.clone()),
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        // Query from child - should prefer exact match over ancestor
        let actual = service
            .find_workspace_by_path(child_path, &user_id)
            .await
            .unwrap();

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().workspace_id, child_workspace.workspace_id);
    }

    #[tokio::test]
    async fn test_find_workspace_by_path_returns_closest_ancestor() {
        let temp_dir = tempfile::tempdir().unwrap();
        let grandparent_path = temp_dir.path().canonicalize().unwrap();
        let user_id = UserId::generate();

        // Create nested directories
        let parent_dir = grandparent_path.join("src");
        let child_dir = parent_dir.join("components");
        std::fs::create_dir_all(&child_dir).unwrap();
        let parent_path = parent_dir.canonicalize().unwrap();
        let child_path = child_dir.canonicalize().unwrap();

        let grandparent_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: grandparent_path.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let parent_workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: parent_path.clone(),
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            workspace: Some(parent_workspace.clone()), // Closest ancestor
            ancestor_workspace: Some(grandparent_workspace.clone()),
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        // Query from child - should find closest ancestor (parent, not grandparent)
        let actual = service
            .find_workspace_by_path(child_path, &user_id)
            .await
            .unwrap();

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().workspace_id, parent_workspace.workspace_id);
    }

    #[tokio::test]
    async fn test_find_workspace_by_path_no_match_for_sibling() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path().canonicalize().unwrap();
        let user_id = UserId::generate();

        // Create sibling directories
        let workspace_dir = root_path.join("project1");
        let sibling_dir = root_path.join("project2");
        std::fs::create_dir_all(&workspace_dir).unwrap();
        std::fs::create_dir_all(&sibling_dir).unwrap();
        let workspace_path = workspace_dir.canonicalize().unwrap();
        let sibling_path = sibling_dir.canonicalize().unwrap();

        let workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: workspace_path,
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            workspace: Some(workspace.clone()),
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        // Query sibling directory - should not match
        let actual = service
            .find_workspace_by_path(sibling_path, &user_id)
            .await
            .unwrap();

        assert!(actual.is_none());
    }

    #[test]
    fn test_should_retry_grpc() {
        // Transient errors - should retry
        assert!(should_retry_grpc(&anyhow::Error::new(
            tonic::Status::cancelled("cancelled")
        )));
        assert!(should_retry_grpc(&anyhow::Error::new(
            tonic::Status::unknown("unknown")
        )));
        assert!(should_retry_grpc(&anyhow::Error::new(
            tonic::Status::deadline_exceeded("timeout")
        )));
        assert!(should_retry_grpc(&anyhow::Error::new(
            tonic::Status::resource_exhausted("rate limited")
        )));
        assert!(should_retry_grpc(&anyhow::Error::new(
            tonic::Status::aborted("aborted")
        )));
        assert!(should_retry_grpc(&anyhow::Error::new(
            tonic::Status::unavailable("unavailable")
        )));

        // Non-gRPC errors - should retry by default
        assert!(should_retry_grpc(&anyhow::anyhow!("generic error")));

        // Permanent errors - should NOT retry
        assert!(!should_retry_grpc(&anyhow::Error::new(
            tonic::Status::invalid_argument("invalid")
        )));
        assert!(!should_retry_grpc(&anyhow::Error::new(
            tonic::Status::not_found("not found")
        )));
        assert!(!should_retry_grpc(&anyhow::Error::new(
            tonic::Status::already_exists("exists")
        )));
        assert!(!should_retry_grpc(&anyhow::Error::new(
            tonic::Status::permission_denied("denied")
        )));
        assert!(!should_retry_grpc(&anyhow::Error::new(
            tonic::Status::failed_precondition("precondition")
        )));
        assert!(!should_retry_grpc(&anyhow::Error::new(
            tonic::Status::out_of_range("range")
        )));
        assert!(!should_retry_grpc(&anyhow::Error::new(
            tonic::Status::unimplemented("unimplemented")
        )));
        assert!(!should_retry_grpc(&anyhow::Error::new(
            tonic::Status::internal("internal")
        )));
        assert!(!should_retry_grpc(&anyhow::Error::new(
            tonic::Status::unauthenticated("unauthenticated")
        )));
        assert!(!should_retry_grpc(&anyhow::Error::new(
            tonic::Status::data_loss("data loss")
        )));

        // Connection errors - should NOT retry (service offline)
        assert!(!should_retry_grpc(&anyhow::anyhow!(
            "Connection refused (os error 61)"
        )));
        assert!(!should_retry_grpc(&anyhow::anyhow!(
            "tcp connect error: Connection refused"
        )));
        assert!(!should_retry_grpc(&anyhow::anyhow!(
            "transport error: tcp connect error"
        )));
    }

    #[test]
    fn test_should_retry_grpc_unavailable_with_connection_refused() {
        // Test the exact scenario from the bug report:
        // Status code is Unavailable, but the underlying error is a connection refused
        // This should NOT retry because the service is offline

        // Simulate the actual error message format from tonic transport errors
        let error = anyhow::anyhow!(
            "code: 'The service is currently unavailable', message: \"tcp connect error\", \
            source: tonic::transport::Error(Transport, ConnectError(ConnectError(\
            \"tcp connect error\", 127.0.0.1:8080, Os {{ code: 61, kind: ConnectionRefused, \
            message: \"Connection refused\" }})))"
        );

        assert!(
            !should_retry_grpc(&error),
            "Expected connection refused error to NOT be retried"
        );

        // Test with wrapped tonic::Status::unavailable containing connection error
        let error = anyhow::Error::new(tonic::Status::unavailable("tcp connect error"))
            .context("Connection refused (os error 61)");

        assert!(
            !should_retry_grpc(&error),
            "Expected unavailable status with connection refused to NOT be retried"
        );

        // Test case-insensitive matching for connection errors
        let error = anyhow::anyhow!("CONNECTION REFUSED (os error 61)");
        assert!(
            !should_retry_grpc(&error),
            "Expected uppercase CONNECTION REFUSED to NOT be retried"
        );

        let error = anyhow::anyhow!("TCP Connect Error: connection failed");
        assert!(
            !should_retry_grpc(&error),
            "Expected mixed case TCP Connect Error to NOT be retried"
        );

        let error = anyhow::anyhow!("Transport error: Tcp Connect Error");
        assert!(
            !should_retry_grpc(&error),
            "Expected title case Tcp Connect Error to NOT be retried"
        );

        // Verify that normal unavailable errors (without connection issues) ARE retried
        let error = anyhow::Error::new(tonic::Status::unavailable("service temporarily down"));

        assert!(
            should_retry_grpc(&error),
            "Expected normal unavailable status to be retried"
        );
    }

    #[tokio::test]
    async fn test_find_workspace_by_path_similar_prefix_no_match() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path().canonicalize().unwrap();
        let user_id = UserId::generate();

        // Create directories with similar prefixes
        let short_dir = root_path.join("pro");
        let long_dir = root_path.join("project");
        std::fs::create_dir_all(&short_dir).unwrap();
        std::fs::create_dir_all(&long_dir).unwrap();
        let short_path = short_dir.canonicalize().unwrap();
        let long_path = long_dir.canonicalize().unwrap();

        let workspace = Workspace {
            workspace_id: WorkspaceId::generate(),
            user_id: user_id.clone(),
            path: short_path,
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let mock = MockInfra {
            workspace: Some(workspace.clone()),
            authenticated: true,
            ..Default::default()
        };

        let service = ForgeWorkspaceService::new(Arc::new(mock));

        // Query directory with similar prefix - should not match (pro vs project)
        let actual = service
            .find_workspace_by_path(long_path, &user_id)
            .await
            .unwrap();

        assert!(actual.is_none());
    }
}
