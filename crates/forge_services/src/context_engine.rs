use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_app::{ContextEngineService, FileReaderInfra, Walker, WalkerInfra, compute_hash};
use forge_domain::{
    AuthCredential, ContextEngineRepository, FileHash, ProviderId, ProviderRepository,
    SyncProgress, UserId, WorkspaceId, WorkspaceRepository,
};
use forge_stream::MpscStream;
use futures::future::join_all;
use tracing::{info, warn};

/// Boxed future type for async closures.
type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

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

/// Result of comparing local and server files
struct SyncPlan {
    /// Files to delete from server (outdated or orphaned)
    files_to_delete: Vec<String>,
    /// Files to upload (new or changed)
    files_to_upload: Vec<forge_domain::FileRead>,
    /// Files that are modified (exists in both delete and upload)
    modified_files: std::collections::HashSet<String>,
}

impl SyncPlan {
    /// Creates a sync plan by comparing local files with remote file hashes.
    fn new(local_files: Vec<IndexedFile>, remote_files: Vec<FileHash>) -> Self {
        // Build hash maps for O(1) lookup
        let local_hashes: HashMap<&str, &str> = local_files
            .iter()
            .map(|f| (f.path.as_str(), f.hash.as_str()))
            .collect();
        let remote_hashes: HashMap<&str, &str> = remote_files
            .iter()
            .map(|f| (f.path.as_str(), f.hash.as_str()))
            .collect();

        // Files to delete: on server but not local or hash changed
        let files_to_delete: Vec<String> = remote_files
            .iter()
            .filter(|f| local_hashes.get(f.path.as_str()) != Some(&f.hash.as_str()))
            .map(|f| f.path.clone())
            .collect();

        // Files to upload: local files not on server or hash changed
        let files_to_upload: Vec<_> = local_files
            .into_iter()
            .filter(|f| remote_hashes.get(f.path.as_str()) != Some(&f.hash.as_str()))
            .map(|f| forge_domain::FileRead::new(f.path, f.content))
            .collect();

        // Modified files: paths that appear in both delete and upload lists
        let delete_paths: std::collections::HashSet<&str> =
            files_to_delete.iter().map(|s| s.as_str()).collect();
        let modified_files: std::collections::HashSet<String> = files_to_upload
            .iter()
            .filter(|f| delete_paths.contains(f.path.as_str()))
            .map(|f| f.path.clone())
            .collect();

        Self { files_to_delete, files_to_upload, modified_files }
    }

    /// Returns the total file count. Modified files count as 1 (not 2
    /// operations).
    fn total(&self) -> usize {
        self.files_to_delete.len() + self.files_to_upload.len() - self.modified_files.len()
    }

    /// Calculates the score contribution for a batch of paths.
    /// Modified files contribute 0.5 (half for delete, half for upload).
    /// Non-modified files contribute 1.0.
    fn batch_score<'a>(&self, paths: impl Iterator<Item = &'a str>) -> f64 {
        paths
            .map(|path| {
                if self.modified_files.contains(path) {
                    0.5
                } else {
                    1.0
                }
            })
            .sum()
    }

    /// Executes the sync plan in batches, consuming self.
    /// Progress is reported as (current_score, total) where modified files
    /// contribute 0.5 for delete and 0.5 for upload.
    async fn execute<'a>(
        self,
        batch_size: usize,
        delete: impl Fn(Vec<String>) -> BoxFuture<'a, Result<()>>,
        upload: impl Fn(Vec<forge_domain::FileRead>) -> BoxFuture<'a, Result<()>>,
        on_progress: impl Fn(f64, usize) -> BoxFuture<'a, ()>,
    ) -> Result<()> {
        let total = self.total();
        if total == 0 {
            return Ok(());
        }

        let mut current_score = 0.0;
        on_progress(current_score, total).await;

        // Delete outdated/orphaned files
        for batch in self.files_to_delete.chunks(batch_size) {
            delete(batch.to_vec()).await?;
            current_score += self.batch_score(batch.iter().map(|s| s.as_str()));
            on_progress(current_score, total).await;
        }

        // Upload new/changed files
        for batch in self.files_to_upload.chunks(batch_size) {
            upload(batch.to_vec()).await?;
            current_score += self.batch_score(batch.iter().map(|f| f.path.as_str()));
            on_progress(current_score, total).await;
        }

        Ok(())
    }
}

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

        // Emit starting event
        emit(SyncProgress::Starting).await;

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
            // Create an workspace.
            let id = self
                .infra
                .create_workspace(&canonical_path, &token)
                .await
                .context("Failed to create workspace on server")?;

            // Save workspace in database to avoid creating multiple workspaces
            self.infra
                .upsert(&id, &user_id, &canonical_path)
                .await
                .context("Failed to save workspace")?;

            emit(SyncProgress::WorkspaceCreated { workspace_id: id.clone() }).await;
            id
        } else {
            workspace_id
        };

        // Read all files and compute hashes
        emit(SyncProgress::DiscoveringFiles { path: canonical_path.clone() }).await;
        let local_files = self.read_files(&canonical_path).await?;
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

        // Fetch remote hashes and create sync plan
        let plan = SyncPlan::new(local_files, remote_files);
        let uploaded_files = plan.total();

        // Only emit diff computed event if there are actual changes
        if !plan.files_to_delete.is_empty()
            || !plan.files_to_upload.is_empty()
            || !plan.modified_files.is_empty()
        {
            emit(SyncProgress::DiffComputed {
                to_delete: plan.files_to_delete.len(),
                to_upload: plan.files_to_upload.len(),
                modified: plan.modified_files.len(),
            })
            .await;
        }

        plan.execute(
            batch_size,
            |paths| {
                let user_id = user_id.clone();
                let workspace_id = workspace_id.clone();
                let token = token.clone();
                Box::pin(async move { self.delete(&user_id, &workspace_id, &token, paths).await })
            },
            |files| {
                let user_id = user_id.clone();
                let workspace_id = workspace_id.clone();
                let token = token.clone();
                Box::pin(async move { self.upload(&user_id, &workspace_id, &token, files).await })
            },
            |current, total| {
                let emit = &emit;
                Box::pin(async move { emit(SyncProgress::Syncing { current, total }).await })
            },
        )
        .await?;

        // Save workspace metadata
        self.infra
            .upsert(&workspace_id, &user_id, &canonical_path)
            .await
            .context("Failed to save workspace")?;

        info!(
            workspace_id = %workspace_id,
            total_files = total_file_count,
            "Sync completed successfully"
        );

        emit(SyncProgress::Completed { total_files: total_file_count, uploaded_files }).await;

        Ok(())
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
        + FileReaderInfra
        + forge_domain::SkillRepository
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

    async fn recommend_skills(
        &self,
        use_case: String,
    ) -> Result<Vec<forge_domain::SelectedSkill>> {
        // Get auth token and skills in parallel
        let (credential, skills) = tokio::join!(
            self.infra.get_credential(&ProviderId::FORGE_SERVICES),
            self.infra.load_skills()
        );

        let token = match credential?
            .ok_or(forge_domain::Error::AuthTokenNotFound)?
            .auth_details
        {
            forge_domain::AuthDetails::ApiKey(token) => token,
            _ => anyhow::bail!("ForgeServices credential must be an API key"),
        };

        let skill_infos: Vec<_> = skills?
            .iter()
            .map(|s| forge_domain::SkillInfo::new(&s.name, &s.description))
            .collect();

        // Build params and call the repository
        let params = forge_domain::SkillSelectionParams::new(skill_infos, use_case);
        self.infra
            .select_skill(params, &token)
            .await
            .context("Failed to select skills")
    }
}

// Additional authentication methods for ForgeIndexingService
impl<F> ForgeContextEngineService<F>
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

        async fn select_skill(
            &self,
            _params: forge_domain::SkillSelectionParams,
            _token: &ApiKey,
        ) -> Result<Vec<forge_domain::SelectedSkill>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl forge_domain::SkillRepository for MockInfra {
        async fn load_skills(&self) -> Result<Vec<forge_domain::Skill>> {
            Ok(vec![])
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
        let service = ForgeContextEngineService::new(Arc::new(MockInfra::default()));

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
            node_count: 0,
            relation_count: 0,
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
            node_count: 5,
            relation_count: 10,
            last_updated: Some(chrono::Utc::now()),
            created_at: chrono::Utc::now(),
        });
        let service = ForgeContextEngineService::new(Arc::new(mock));

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
                .contains("No indexing authentication found")
        );
    }

    #[test]
    fn test_sync_plan_new_computes_correct_diff() {
        let local = vec![
            IndexedFile::new("a.rs".into(), "content_a".into(), "hash_a".into()),
            IndexedFile::new("b.rs".into(), "new_content".into(), "new_hash".into()),
            IndexedFile::new("d.rs".into(), "content_d".into(), "hash_d".into()),
        ];
        let remote = vec![
            FileHash { path: "a.rs".into(), hash: "hash_a".into() },
            FileHash { path: "b.rs".into(), hash: "old_hash".into() },
            FileHash { path: "c.rs".into(), hash: "hash_c".into() },
        ];

        let actual = SyncPlan::new(local, remote);

        // b.rs is modified (in both delete and upload), c.rs is orphaned, d.rs is new
        // total = 2 deletes + 2 uploads - 1 modified = 3 files
        assert_eq!(actual.total(), 3);
        assert_eq!(actual.files_to_delete.len(), 2);
        assert_eq!(actual.files_to_upload.len(), 2);
        assert_eq!(actual.modified_files.len(), 1);
        assert!(actual.files_to_delete.contains(&"b.rs".to_string()));
        assert!(actual.files_to_delete.contains(&"c.rs".to_string()));
        assert!(actual.files_to_upload.iter().any(|f| f.path == "b.rs"));
        assert!(actual.files_to_upload.iter().any(|f| f.path == "d.rs"));
        assert!(actual.modified_files.contains("b.rs"));
    }

    #[tokio::test]
    async fn test_sync_plan_execute_batches_and_tracks_progress() {
        use std::sync::Mutex;

        // No modified files: 3 pure deletes + 2 pure uploads = 5 total files
        let plan = SyncPlan {
            files_to_delete: vec!["a.rs".into(), "b.rs".into(), "c.rs".into()],
            files_to_upload: vec![
                forge_domain::FileRead::new("d.rs".into(), "content_d".into()),
                forge_domain::FileRead::new("e.rs".into(), "content_e".into()),
            ],
            modified_files: std::collections::HashSet::new(),
        };

        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = progress.clone();

        plan.execute(
            2,
            |_| Box::pin(async { Ok(()) }),
            |_| Box::pin(async { Ok(()) }),
            move |processed, total| {
                let progress = progress_clone.clone();
                Box::pin(async move {
                    progress.lock().unwrap().push((processed, total));
                })
            },
        )
        .await
        .unwrap();

        let actual = progress.lock().unwrap().clone();
        // Batch 1: delete a.rs, b.rs -> score 2.0
        // Batch 2: delete c.rs -> score 3.0
        // Batch 3: upload d.rs, e.rs -> score 5.0
        let expected = vec![(0.0, 5), (2.0, 5), (3.0, 5), (5.0, 5)];

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_sync_plan_execute_with_modified_files() {
        use std::sync::Mutex;

        // Scenario:
        // - a.rs: modified (delete old + upload new) -> 0.5 + 0.5 = 1.0
        // - b.rs: pure delete (orphaned) -> 1.0
        // - c.rs: pure upload (new file) -> 1.0
        // Total files: 2 deletes + 2 uploads - 1 modified = 3
        let plan = SyncPlan {
            files_to_delete: vec!["a.rs".into(), "b.rs".into()],
            files_to_upload: vec![
                forge_domain::FileRead::new("a.rs".into(), "new_content".into()),
                forge_domain::FileRead::new("c.rs".into(), "content_c".into()),
            ],
            modified_files: ["a.rs".to_string()].into_iter().collect(),
        };

        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = progress.clone();

        plan.execute(
            1, // batch size 1 to see each operation
            |_| Box::pin(async { Ok(()) }),
            |_| Box::pin(async { Ok(()) }),
            move |score, total| {
                let progress = progress_clone.clone();
                Box::pin(async move {
                    progress.lock().unwrap().push((score, total));
                })
            },
        )
        .await
        .unwrap();

        let actual = progress.lock().unwrap().clone();
        // Initial: 0.0
        // Delete a.rs (modified): +0.5 -> 0.5
        // Delete b.rs (pure): +1.0 -> 1.5
        // Upload a.rs (modified): +0.5 -> 2.0
        // Upload c.rs (pure): +1.0 -> 3.0
        let expected = vec![
            (0.0, 3), // initial
            (0.5, 3), // after delete a.rs (modified)
            (1.5, 3), // after delete b.rs (pure)
            (2.0, 3), // after upload a.rs (modified)
            (3.0, 3), // after upload c.rs (pure)
        ];

        assert_eq!(actual, expected);
    }
}
