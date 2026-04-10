use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_app::{CommandInfra, EnvironmentInfra, FileReaderInfra, WalkerInfra, WorkspaceService};
use forge_domain::{
    AuthCredential, AuthDetails, ProviderId, ProviderRepository, SyncProgress, UserId, WorkspaceId,
    WorkspaceIndexRepository,
};
use forge_stream::MpscStream;
use futures::future::join_all;
use tracing::info;

use crate::fd::FileDiscovery;
use crate::sync::{WorkspaceSyncEngine, canonicalize_path};

/// Service for indexing workspaces and performing semantic search.
///
/// `F` provides infrastructure capabilities (file I/O, environment, etc.) and
/// `D` is the file-discovery strategy used to enumerate workspace files.
pub struct ForgeWorkspaceService<F, D> {
    infra: Arc<F>,
    discovery: Arc<D>,
}

impl<F, D> Clone for ForgeWorkspaceService<F, D> {
    fn clone(&self) -> Self {
        Self {
            infra: Arc::clone(&self.infra),
            discovery: Arc::clone(&self.discovery),
        }
    }
}

impl<F, D> ForgeWorkspaceService<F, D> {
    /// Creates a new workspace service with the provided infrastructure and
    /// file-discovery strategy.
    pub fn new(infra: Arc<F>, discovery: Arc<D>) -> Self {
        Self { infra, discovery }
    }
}

impl<
    F: 'static
        + ProviderRepository
        + WorkspaceIndexRepository
        + FileReaderInfra
        + EnvironmentInfra<Config = forge_config::ForgeConfig>
        + CommandInfra
        + WalkerInfra,
    D: FileDiscovery + 'static,
> ForgeWorkspaceService<F, D>
{
    /// Internal sync implementation that emits progress events.
    async fn sync_codebase_internal<E, Fut>(&self, path: PathBuf, emit: E) -> Result<()>
    where
        E: Fn(SyncProgress) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = ()> + Send,
    {
        info!(path = %path.display(), "Starting workspace sync");

        emit(SyncProgress::Starting).await;

        let (token, user_id) = self.get_workspace_credentials().await?;
        let batch_size = self.infra.get_config()?.max_file_read_batch_size;
        let path = canonicalize_path(path)?;

        // Find existing workspace - do NOT auto-create
        let workspace = self.get_workspace_by_path(path, &token).await?;
        let workspace_id = workspace.workspace_id.clone();

        // Use the canonical root stored in the workspace record so that file
        // discovery and remote-hash comparison are always relative to the same
        // base, even when `path` is a subdirectory of an ancestor workspace.
        let workspace_root = PathBuf::from(&workspace.working_dir);

        WorkspaceSyncEngine::new(
            Arc::clone(&self.infra),
            Arc::clone(&self.discovery),
            workspace_root,
            workspace_id,
            user_id,
            token,
            batch_size,
        )
        .run(emit)
        .await
    }

    /// Gets the ForgeCode services credential and extracts workspace auth
    /// components
    ///
    /// # Errors
    /// Returns an error if the credential is not found, if there's a database
    /// error, or if the credential format is invalid
    async fn get_workspace_credentials(&self) -> Result<(forge_domain::ApiKey, UserId)> {
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

    /// Finds a workspace by path from remote server, checking for exact match
    /// first, then ancestor workspaces.
    ///
    /// Business logic:
    /// 1. First tries to find an exact match for the given path
    /// 2. If not found, searches for ancestor workspaces
    /// 3. Returns the closest ancestor (longest matching path prefix)
    ///
    /// # Errors
    /// Returns an error if the path cannot be canonicalized or if there's a
    /// server error. Returns Ok(None) if no workspace is found.
    async fn find_workspace_by_path(
        &self,
        path: PathBuf,
        token: &forge_domain::ApiKey,
    ) -> Result<Option<forge_domain::WorkspaceInfo>> {
        let canonical_path = canonicalize_path(path)?;

        // Get all workspaces from remote server
        let workspaces = self.infra.list_workspaces(token).await?;

        let canonical_str = canonical_path.to_string_lossy();

        // Business logic: choose which workspace to use
        // 1. First check for exact match
        if let Some(exact_match) = workspaces.iter().find(|w| w.working_dir == canonical_str) {
            return Ok(Some(exact_match.clone()));
        }

        // 2. Find closest ancestor (longest matching path prefix)
        let mut best_match: Option<(&forge_domain::WorkspaceInfo, usize)> = None;

        for workspace in &workspaces {
            let workspace_path = PathBuf::from(&workspace.working_dir);
            if canonical_path.starts_with(&workspace_path) {
                let path_len = workspace.working_dir.len();
                if best_match.is_none_or(|(_, len)| path_len > len) {
                    best_match = Some((workspace, path_len));
                }
            }
        }

        Ok(best_match.map(|(w, _)| w.clone()))
    }

    /// Looks up the workspace for `path` and returns it, or an error if no
    /// workspace has been indexed for that path.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying repository lookup fails, or when no
    /// matching workspace is found (i.e. the workspace has not been indexed
    /// yet).
    async fn get_workspace_by_path(
        &self,
        path: PathBuf,
        token: &forge_domain::ApiKey,
    ) -> Result<forge_domain::WorkspaceInfo> {
        self.find_workspace_by_path(path, token)
            .await?
            .context("Workspace not indexed. Please run `forge workspace init` first.")
    }

    async fn _init_workspace(&self, path: PathBuf) -> Result<(bool, WorkspaceId)> {
        let (token, _user_id) = self.get_workspace_credentials().await?;
        let path = canonicalize_path(path)?;

        // Find workspace by exact match or ancestor from remote server
        let workspace = self.find_workspace_by_path(path.clone(), &token).await?;

        let (workspace_id, workspace_path, is_new_workspace) = match workspace {
            Some(workspace_info) => {
                // Found existing workspace - reuse it
                (workspace_info.workspace_id, path.clone(), false)
            }
            None => {
                // No workspace found - create new
                (WorkspaceId::generate(), path.clone(), true)
            }
        };

        let workspace_id = if is_new_workspace {
            // Create workspace on server
            self.infra
                .create_workspace(&workspace_path, &token)
                .await
                .context("Failed to create workspace on server")?
        } else {
            workspace_id
        };

        Ok((is_new_workspace, workspace_id))
    }
}

#[async_trait]
impl<
    F: ProviderRepository
        + WorkspaceIndexRepository
        + FileReaderInfra
        + EnvironmentInfra<Config = forge_config::ForgeConfig>
        + CommandInfra
        + WalkerInfra
        + 'static,
    D: FileDiscovery + 'static,
> WorkspaceService for ForgeWorkspaceService<F, D>
{
    async fn sync_workspace(&self, path: PathBuf) -> Result<MpscStream<Result<SyncProgress>>> {
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
            let result = service.sync_codebase_internal(path, emit).await;

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
        mut params: forge_domain::SearchParams<'_>,
    ) -> Result<Vec<forge_domain::Node>> {
        let (token, user_id) = self.get_workspace_credentials().await?;

        let workspace = self
            .find_workspace_by_path(path, &token)
            .await?
            .ok_or(forge_domain::Error::WorkspaceNotFound)?;

        let max_limit = self.infra.get_config()?.max_sem_search_results;
        params.limit = match params.limit {
            Some(l) if l <= max_limit => Some(l),
            _ => Some(max_limit),
        };

        let search_query =
            forge_domain::CodeBase::new(user_id, workspace.workspace_id.clone(), params);

        let results = self
            .infra
            .search(&search_query, &token)
            .await
            .context("Failed to search")?;

        Ok(results)
    }

    /// Lists all workspaces.
    async fn list_workspaces(&self) -> Result<Vec<forge_domain::WorkspaceInfo>> {
        let (token, _) = self.get_workspace_credentials().await?;

        self.infra
            .as_ref()
            .list_workspaces(&token)
            .await
            .context("Failed to list workspaces")
    }

    /// Retrieves workspace information for a specific path.
    async fn get_workspace_info(
        &self,
        path: PathBuf,
    ) -> Result<Option<forge_domain::WorkspaceInfo>> {
        let (token, _user_id) = self.get_workspace_credentials().await?;
        let workspace = self.find_workspace_by_path(path, &token).await?;

        Ok(workspace)
    }

    /// Deletes a workspace from the server.
    async fn delete_workspace(&self, workspace_id: &forge_domain::WorkspaceId) -> Result<()> {
        let (token, _) = self.get_workspace_credentials().await?;

        self.infra
            .as_ref()
            .delete_workspace(workspace_id, &token)
            .await
            .context("Failed to delete workspace from server")?;

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
        let (token, _user_id) = self.get_workspace_credentials().await?;
        match self
            .find_workspace_by_path(path.to_path_buf(), &token)
            .await
        {
            Ok(workspace) => Ok(workspace.is_some()),
            Err(_) => Ok(false), // Path doesn't exist or other error, so it can't be indexed
        }
    }

    async fn get_workspace_status(&self, path: PathBuf) -> Result<Vec<forge_domain::FileStatus>> {
        let (token, user_id) = self.get_workspace_credentials().await?;

        let workspace = self.get_workspace_by_path(path, &token).await?;

        // Reuse the canonical path already stored in the workspace (resolved during
        // sync), avoiding a redundant canonicalize() IO call.
        let canonical_path = PathBuf::from(&workspace.working_dir);

        let batch_size = self.infra.get_config()?.max_file_read_batch_size;

        WorkspaceSyncEngine::new(
            Arc::clone(&self.infra),
            Arc::clone(&self.discovery),
            canonical_path,
            workspace.workspace_id,
            user_id,
            token,
            batch_size,
        )
        .compute_status()
        .await
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

    async fn init_workspace(&self, path: PathBuf) -> Result<WorkspaceId> {
        let (is_new, workspace_id) = self._init_workspace(path).await?;

        if is_new {
            Ok(workspace_id)
        } else {
            Err(forge_domain::Error::WorkspaceAlreadyInitialized(workspace_id).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::collections::{BTreeMap, HashMap};
    use futures::stream::Stream;
    use std::pin::Pin;

    use anyhow::Result;
    use async_trait::async_trait;
    use forge_app::{CommandInfra, EnvironmentInfra, FileReaderInfra, WalkerInfra, WalkedFile, Walker, WorkspaceService};
    use forge_domain::{
        AnyProvider, AuthCredential, AuthDetails, CodeSearchQuery, CommandOutput,
        ConfigOperation, Environment, FileHash, FileUpload, FileUploadInfo, MigrationResult, Node,
        ProviderId, ProviderTemplate, WorkspaceAuth, WorkspaceId, WorkspaceInfo,
        ApiKey, UserId, SearchParams, WorkspaceFiles, FileDeletion, FileInfo
    };
    use forge_config::ForgeConfig;
    use fake::{Fake, Faker};

    use crate::fd::FileDiscovery;
    use super::ForgeWorkspaceService;
    use pretty_assertions::assert_eq;

    #[derive(Clone)]
    struct MockInfra {
        pub workspaces: Vec<WorkspaceInfo>,
        pub search_limit: Arc<std::sync::Mutex<Option<usize>>>,
        pub max_sem_search_results: usize,
    }

    impl MockInfra {
        fn new(max_sem_search_results: usize) -> Self {
            Self {
                workspaces: vec![],
                search_limit: Arc::new(std::sync::Mutex::new(None)),
                max_sem_search_results,
            }
        }
    }

    #[async_trait]
    impl forge_domain::ProviderRepository for MockInfra {
        async fn get_all_providers(&self) -> Result<Vec<AnyProvider>> { Ok(vec![]) }
        async fn get_provider(&self, _id: ProviderId) -> Result<ProviderTemplate> { Err(anyhow::anyhow!("unimplemented")) }
        async fn upsert_credential(&self, _c: AuthCredential) -> Result<()> { Ok(()) }
        async fn get_credential(&self, id: &ProviderId) -> Result<Option<AuthCredential>> {
            if id == &ProviderId::FORGE_SERVICES {
                let mut url_params = HashMap::new();
                let user_id = UserId::generate().to_string();
                url_params.insert("user_id".to_string().into(), user_id.into());
                Ok(Some(AuthCredential {
                    id: ProviderId::FORGE_SERVICES,
                    auth_details: AuthDetails::ApiKey(ApiKey::from("test-token".to_string())),
                    url_params,
                }))
            } else {
                Ok(None)
            }
        }
        async fn remove_credential(&self, _id: &ProviderId) -> Result<()> { Ok(()) }
        async fn migrate_env_credentials(&self) -> Result<Option<MigrationResult>> { Ok(None) }
    }

    #[async_trait]
    impl forge_domain::WorkspaceIndexRepository for MockInfra {
        async fn authenticate(&self) -> Result<WorkspaceAuth> { Err(anyhow::anyhow!("unimplemented")) }
        async fn create_workspace(&self, _dir: &Path, _token: &ApiKey) -> Result<WorkspaceId> { Err(anyhow::anyhow!("unimplemented")) }
        async fn upload_files(&self, _u: &FileUpload, _t: &ApiKey) -> Result<FileUploadInfo> { Ok(FileUploadInfo::new(0, 0)) }
        async fn search(&self, q: &CodeSearchQuery<'_>, _t: &ApiKey) -> Result<Vec<Node>> {
            *self.search_limit.lock().unwrap() = q.data.limit;
            Ok(vec![])
        }
        async fn list_workspaces(&self, _t: &ApiKey) -> Result<Vec<WorkspaceInfo>> {
            Ok(self.workspaces.clone())
        }
        async fn get_workspace(&self, _id: &WorkspaceId, _t: &ApiKey) -> Result<Option<WorkspaceInfo>> { Err(anyhow::anyhow!("unimplemented")) }
        async fn list_workspace_files(&self, _id: &WorkspaceFiles, _t: &ApiKey) -> Result<Vec<FileHash>> { Err(anyhow::anyhow!("unimplemented")) }
        async fn delete_files(&self, _d: &FileDeletion, _t: &ApiKey) -> Result<()> { Ok(()) }
        async fn delete_workspace(&self, _id: &WorkspaceId, _t: &ApiKey) -> Result<()> { Ok(()) }
    }

    #[async_trait]
    impl FileReaderInfra for MockInfra {
        async fn read(&self, _p: &Path) -> Result<Vec<u8>> { Ok(vec![]) }
        async fn read_utf8(&self, _p: &Path) -> Result<String> { Ok("".into()) }
        fn read_batch_utf8(&self, _limit: usize, _paths: Vec<PathBuf>) -> impl Stream<Item = (PathBuf, Result<String>)> + Send {
            futures::stream::empty()
        }
        async fn range_read_utf8(&self, _p: &Path, _s: u64, _e: u64) -> Result<(String, FileInfo)> { Err(anyhow::anyhow!("unimplemented")) }
    }

    impl EnvironmentInfra for MockInfra {
        type Config = ForgeConfig;
        fn get_environment(&self) -> Environment { Faker.fake() }
        fn get_config(&self) -> Result<ForgeConfig> {
            let mut cfg = ForgeConfig::default();
            cfg.max_sem_search_results = self.max_sem_search_results;
            Ok(cfg)
        }
        async fn update_environment(&self, _ops: Vec<ConfigOperation>) -> Result<()> { Ok(()) }
        fn get_env_var(&self, _k: &str) -> Option<String> { None }
        fn get_env_vars(&self) -> BTreeMap<String, String> { BTreeMap::new() }
    }

    #[async_trait]
    impl CommandInfra for MockInfra {
        async fn execute_command(
            &self,
            command: String,
            _working_dir: PathBuf,
            _keep_ansi: bool,
            
            _env_vars: Option<Vec<String>>,
        ) -> Result<CommandOutput> { Ok(CommandOutput { command, exit_code: Some(0), stdout: "".into(), stderr: "".into() }) }
        async fn execute_command_raw(
            &self,
            _command: &str,
            _working_dir: PathBuf,
            _env_vars: Option<Vec<String>>,
        ) -> Result<std::process::ExitStatus> { Err(anyhow::anyhow!("unimplemented")) }
    }

    #[async_trait]
    impl WalkerInfra for MockInfra {
        async fn walk(&self, _c: Walker) -> Result<Vec<WalkedFile>> { Ok(vec![]) }
    }

    struct MockDiscovery;
    #[async_trait]
    impl FileDiscovery for MockDiscovery {
        async fn discover(&self, _p: &Path) -> Result<Vec<PathBuf>> { Ok(vec![]) }
    }

    #[tokio::test]
    async fn test_is_indexed_not_found() {
        let infra = Arc::new(MockInfra::new(10));
        let discovery = Arc::new(MockDiscovery);
        let service = ForgeWorkspaceService::new(infra, discovery);

        let result = service.is_indexed(Path::new("/definitely/does/not/exist/ever/12345")).await.unwrap();
        assert_eq!(result, false);
    }

    #[tokio::test]
    async fn test_query_workspace_enforces_limit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let canonical_path = temp_dir.path().canonicalize().unwrap();

        let mut infra_val = MockInfra::new(15);
        infra_val.workspaces.push(WorkspaceInfo {
            workspace_id: WorkspaceId::generate(),
            working_dir: canonical_path.to_string_lossy().to_string(),
            node_count: None,
            relation_count: None,
            last_updated: None,
            created_at: chrono::Utc::now(),
        });

        let infra = Arc::new(infra_val);
        let discovery = Arc::new(MockDiscovery);
        let service = ForgeWorkspaceService::new(infra.clone(), discovery);

        // Limit > config max (20 > 15) -> clamped to 15
        let mut params = SearchParams::new("query", "use_case");
        params.limit = Some(20);
        let _ = service.query_workspace(temp_dir.path().to_path_buf(), params.clone()).await.unwrap();
        assert_eq!(*infra.search_limit.lock().unwrap(), Some(15));

        // Limit < config max (10 < 15) -> kept at 10
        params.limit = Some(10);
        let _ = service.query_workspace(temp_dir.path().to_path_buf(), params.clone()).await.unwrap();
        assert_eq!(*infra.search_limit.lock().unwrap(), Some(10));

        // No limit -> uses config max (15)
        params.limit = None;
        let _ = service.query_workspace(temp_dir.path().to_path_buf(), params.clone()).await.unwrap();
        assert_eq!(*infra.search_limit.lock().unwrap(), Some(15));
    }

    #[tokio::test]
    async fn test_workspace_credentials_extraction() {
        let infra = Arc::new(MockInfra::new(10));
        let discovery = Arc::new(MockDiscovery);
        let service = ForgeWorkspaceService::new(infra, discovery);

        let (token, _user_id) = service.get_workspace_credentials().await.unwrap();
        assert_eq!(token, forge_domain::ApiKey::from("test-token".to_string()));
    }

    #[tokio::test]
    async fn test_sync_workspace_emits_starting() {
        let temp_dir = tempfile::tempdir().unwrap();
        let canonical_path = temp_dir.path().canonicalize().unwrap();

        let mut infra_val = MockInfra::new(15);
        infra_val.workspaces.push(WorkspaceInfo {
            workspace_id: WorkspaceId::generate(),
            working_dir: canonical_path.to_string_lossy().to_string(),
            node_count: None,
            relation_count: None,
            last_updated: None,
            created_at: chrono::Utc::now(),
        });

        let infra = Arc::new(infra_val);
        let discovery = Arc::new(MockDiscovery);
        let service = ForgeWorkspaceService::new(infra.clone(), discovery);

        let mut stream = service.sync_workspace(temp_dir.path().to_path_buf()).await.unwrap();
        
        use futures::stream::StreamExt;
        if let Some(Ok(forge_domain::SyncProgress::Starting)) = stream.next().await {
            // Expected
        } else {
            panic!("Expected SyncProgress::Starting");
        }
    }
}
