use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use forge_app::{
    CommandInfra, EnvironmentInfra, FileReaderInfra, Walker, WalkerInfra, WorkspaceService,
    compute_hash,
};
use forge_domain::{
    AuthCredential, CommandOutput, Environment, FileHash, FileInfo, FileUploadInfo, HttpConfig,
    MigrationResult, ProviderId, RetryConfig, UserId, WorkspaceAuth, WorkspaceId,
    WorkspaceInfo,
};
use forge_services::ForgeWorkspaceService;
use forge_stream::MpscStream;
use futures::{Stream, StreamExt};
use tempfile::TempDir;
use url::Url;

#[tokio::main]
async fn main() -> Result<()> {
    let mode = env::var("FORGE_BENCH_MODE").unwrap_or_else(|_| "sync".to_string());
    let file_count = read_env_usize("FORGE_BENCH_FILE_COUNT", 400);
    let file_bytes = read_env_usize("FORGE_BENCH_FILE_BYTES", 512 * 1024);
    let batch_size = read_env_usize("FORGE_BENCH_BATCH_SIZE", 64);

    let fixture = Fixture::new(file_count, file_bytes, batch_size).await?;
    let service = fixture.service();

    match mode.as_str() {
        "status" => {
            let statuses = service
                .get_workspace_status(fixture.root().to_path_buf())
                .await?;
            anyhow::ensure!(statuses.len() == file_count, "unexpected status count");
            anyhow::ensure!(
                statuses
                    .iter()
                    .all(|status| status.status == forge_domain::SyncStatus::InSync),
                "expected every file to be in sync"
            );
            println!(
                "BENCHMARK_OK mode=status files={} file_bytes={} batch_size={} statuses={}",
                file_count,
                file_bytes,
                batch_size,
                statuses.len()
            );
        }
        "sync" => {
            let mut stream = service
                .sync_workspace(fixture.root().to_path_buf(), batch_size)
                .await?;
            let completed = consume_sync_stream(&mut stream).await?;
            anyhow::ensure!(completed.total_files == file_count, "unexpected total file count");
            anyhow::ensure!(completed.uploaded_files == 0, "expected zero uploaded files");
            anyhow::ensure!(completed.failed_files == 0, "expected zero failed files");
            println!(
                "BENCHMARK_OK mode=sync files={} file_bytes={} batch_size={} total_files={} uploaded_files={} failed_files={}",
                file_count,
                file_bytes,
                batch_size,
                completed.total_files,
                completed.uploaded_files,
                completed.failed_files
            );
        }
        other => anyhow::bail!("unsupported FORGE_BENCH_MODE: {other}"),
    }

    Ok(())
}

fn read_env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

#[derive(Debug)]
struct SyncCompleted {
    total_files: usize,
    uploaded_files: usize,
    failed_files: usize,
}

async fn consume_sync_stream(stream: &mut MpscStream<Result<forge_domain::SyncProgress>>) -> Result<SyncCompleted> {
    let mut completed = None;

    while let Some(progress) = stream.next().await {
        match progress? {
            forge_domain::SyncProgress::Completed {
                total_files,
                uploaded_files,
                failed_files,
            } => {
                completed = Some(SyncCompleted { total_files, uploaded_files, failed_files });
            }
            forge_domain::SyncProgress::WorkspaceCreated { .. }
            | forge_domain::SyncProgress::Starting
            | forge_domain::SyncProgress::DiscoveringFiles { .. }
            | forge_domain::SyncProgress::FilesDiscovered { .. }
            | forge_domain::SyncProgress::ComparingFiles { .. }
            | forge_domain::SyncProgress::DiffComputed { .. }
            | forge_domain::SyncProgress::Syncing { .. } => {}
        }
    }

    completed.context("sync stream finished without a completion event")
}

struct Fixture {
    _temp_dir: TempDir,
    root: PathBuf,
    env: Environment,
    credential: AuthCredential,
    workspace: WorkspaceInfo,
    remote_files: Arc<Vec<FileHash>>,
}

impl Fixture {
    async fn new(file_count: usize, file_bytes: usize, batch_size: usize) -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir.path().join("workspace");
        tokio::fs::create_dir_all(&root).await?;
        let root = tokio::fs::canonicalize(&root).await?;
        init_git_repo(&root).await?;

        let remote_files = generate_files(&root, file_count, file_bytes).await?;
        stage_repo(&root).await?;

        let env = benchmark_environment(&root, batch_size)?;
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();
        let credential = credential(user_id.clone());
        let workspace = WorkspaceInfo {
            workspace_id,
            working_dir: root.to_string_lossy().into_owned(),
            node_count: Some(file_count as u64),
            relation_count: Some(0),
            last_updated: Some(Utc::now()),
            created_at: Utc::now(),
        };

        Ok(Self {
            _temp_dir: temp_dir,
            root,
            env,
            credential,
            workspace,
            remote_files: Arc::new(remote_files),
        })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn service(&self) -> ForgeWorkspaceService<MockInfra> {
        ForgeWorkspaceService::new(Arc::new(MockInfra {
            env: self.env.clone(),
            credential: self.credential.clone(),
            workspace: self.workspace.clone(),
            remote_files: self.remote_files.clone(),
        }))
    }
}

fn benchmark_environment(cwd: &Path, batch_size: usize) -> Result<Environment> {
    Ok(Environment {
        os: env::consts::OS.to_string(),
        pid: std::process::id(),
        cwd: cwd.to_path_buf(),
        home: Some(cwd.to_path_buf()),
        shell: "/bin/bash".to_string(),
        base_path: cwd.join(".forge-bench"),
        forge_api_url: Url::parse("https://example.com")?,
        retry_config: RetryConfig::default(),
        max_search_lines: 1_000,
        max_search_result_bytes: 512 * 1024,
        fetch_truncation_limit: 512 * 1024,
        stdout_max_prefix_length: 128,
        stdout_max_suffix_length: 128,
        stdout_max_line_length: 8_192,
        max_line_length: 8_192,
        max_read_size: 50_000,
        max_file_read_batch_size: batch_size.max(1),
        http: HttpConfig::default(),
        max_file_size: u64::MAX,
        max_image_size: 10 * 1024 * 1024,
        tool_timeout: 60,
        auto_open_dump: false,
        debug_requests: None,
        custom_history_path: None,
        max_conversations: 100,
        sem_search_limit: 10,
        sem_search_top_k: 20,
        workspace_server_url: Url::parse("http://127.0.0.1:1")?,
        max_extensions: 64,
        auto_dump: None,
        parallel_file_reads: batch_size.max(1),
    })
}

fn credential(user_id: UserId) -> AuthCredential {
    let mut credential = AuthCredential::new_api_key(
        ProviderId::FORGE_SERVICES,
        "bench-token".to_string().into(),
    );
    credential
        .url_params
        .insert("user_id".to_string().into(), user_id.to_string().into());
    credential
}

async fn init_git_repo(root: &Path) -> Result<()> {
    let output = tokio::process::Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(root)
        .output()
        .await?;
    anyhow::ensure!(output.status.success(), "git init failed");
    Ok(())
}

async fn stage_repo(root: &Path) -> Result<()> {
    let output = tokio::process::Command::new("git")
        .arg("add")
        .arg(".")
        .current_dir(root)
        .output()
        .await?;
    anyhow::ensure!(output.status.success(), "git add failed");
    Ok(())
}

async fn generate_files(root: &Path, file_count: usize, file_bytes: usize) -> Result<Vec<FileHash>> {
    let mut remote_files = Vec::with_capacity(file_count);

    for index in 0..file_count {
        let directory = root.join("src").join(format!("chunk_{:04}", index / 100));
        tokio::fs::create_dir_all(&directory).await?;

        let relative = PathBuf::from("src")
            .join(format!("chunk_{:04}", index / 100))
            .join(format!("file_{index:05}.rs"));
        let path = root.join(&relative);
        let content = benchmark_file(index, file_bytes);
        tokio::fs::write(&path, &content).await?;

        remote_files.push(FileHash {
            path: relative.to_string_lossy().into_owned(),
            hash: compute_hash(&content),
        });
    }

    Ok(remote_files)
}

fn benchmark_file(index: usize, file_bytes: usize) -> String {
    let header = format!("pub const FILE_{index}: usize = {index};\n");
    let footer = format!("\npub fn checksum() -> usize {{ FILE_{index} }}\n");
    let body_len = file_bytes.saturating_sub(header.len() + footer.len()).max(1);
    let body = "a".repeat(body_len);
    format!("{header}{body}{footer}")
}

#[derive(Clone)]
struct MockInfra {
    env: Environment,
    credential: AuthCredential,
    workspace: WorkspaceInfo,
    remote_files: Arc<Vec<FileHash>>,
}

impl EnvironmentInfra for MockInfra {
    fn get_environment(&self) -> Environment {
        self.env.clone()
    }

    fn get_env_var(&self, _key: &str) -> Option<String> {
        None
    }

    fn get_env_vars(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn is_restricted(&self) -> bool {
        false
    }
}

#[async_trait]
impl FileReaderInfra for MockInfra {
    async fn read_utf8(&self, path: &Path) -> Result<String> {
        Ok(tokio::fs::read_to_string(path).await?)
    }

    fn read_batch_utf8(
        &self,
        batch_size: usize,
        paths: Vec<PathBuf>,
    ) -> impl Stream<Item = (PathBuf, Result<String>)> + Send {
        futures::stream::iter(paths)
            .map(|path| async move {
                let content = tokio::fs::read_to_string(&path)
                    .await
                    .map_err(anyhow::Error::from);
                (path, content)
            })
            .buffer_unordered(batch_size.max(1))
    }

    async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        Ok(tokio::fs::read(path).await?)
    }

    async fn range_read_utf8(&self, path: &Path, start_line: u64, end_line: u64) -> Result<(String, FileInfo)> {
        let content = tokio::fs::read_to_string(path).await?;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len() as u64;
        let start = start_line.saturating_sub(1) as usize;
        let end = end_line.min(total_lines) as usize;
        let slice = if start < end { lines[start..end].join("\n") } else { String::new() };
        Ok((slice, FileInfo::new(start_line, end_line.min(total_lines), total_lines)))
    }
}

#[async_trait]
impl CommandInfra for MockInfra {
    async fn execute_command(
        &self,
        command: String,
        working_dir: PathBuf,
        _silent: bool,
        env_vars: Option<Vec<String>>,
    ) -> Result<CommandOutput> {
        let mut process = tokio::process::Command::new("sh");
        process.arg("-lc").arg(&command).current_dir(working_dir);

        if let Some(env_vars) = env_vars {
            for env_var in env_vars {
                if let Some((key, value)) = env_var.split_once('=') {
                    process.env(key, value);
                }
            }
        }

        let output = process.output().await?;
        Ok(CommandOutput {
            command,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
        })
    }

    async fn execute_command_raw(
        &self,
        command: &str,
        working_dir: PathBuf,
        env_vars: Option<Vec<String>>,
    ) -> Result<std::process::ExitStatus> {
        let mut process = tokio::process::Command::new("sh");
        process.arg("-lc").arg(command).current_dir(working_dir);

        if let Some(env_vars) = env_vars {
            for env_var in env_vars {
                if let Some((key, value)) = env_var.split_once('=') {
                    process.env(key, value);
                }
            }
        }

        Ok(process.status().await?)
    }
}

#[async_trait]
impl WalkerInfra for MockInfra {
    async fn walk(&self, _config: Walker) -> Result<Vec<forge_app::WalkedFile>> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl forge_domain::ProviderRepository for MockInfra {
    async fn get_all_providers(&self) -> Result<Vec<forge_domain::AnyProvider>> {
        Ok(Vec::new())
    }

    async fn get_provider(&self, _id: ProviderId) -> Result<forge_domain::ProviderTemplate> {
        anyhow::bail!("unused in benchmark")
    }

    async fn upsert_credential(&self, _credential: AuthCredential) -> Result<()> {
        Ok(())
    }

    async fn get_credential(&self, id: &ProviderId) -> Result<Option<AuthCredential>> {
        if *id == ProviderId::FORGE_SERVICES {
            Ok(Some(self.credential.clone()))
        } else {
            Ok(None)
        }
    }

    async fn remove_credential(&self, _id: &ProviderId) -> Result<()> {
        Ok(())
    }

    async fn migrate_env_credentials(&self) -> Result<Option<MigrationResult>> {
        Ok(None)
    }
}

#[async_trait]
impl forge_domain::WorkspaceIndexRepository for MockInfra {
    async fn authenticate(&self) -> Result<WorkspaceAuth> {
        Ok(WorkspaceAuth::new(
            UserId::generate(),
            "bench-token".to_string().into(),
        ))
    }

    async fn create_workspace(&self, _working_dir: &Path, _auth_token: &forge_domain::ApiKey) -> Result<WorkspaceId> {
        Ok(self.workspace.workspace_id.clone())
    }

    async fn upload_files(
        &self,
        _upload: &forge_domain::FileUpload,
        _auth_token: &forge_domain::ApiKey,
    ) -> Result<FileUploadInfo> {
        Ok(FileUploadInfo::new(0, 0))
    }

    async fn search(
        &self,
        _query: &forge_domain::CodeSearchQuery<'_>,
        _auth_token: &forge_domain::ApiKey,
    ) -> Result<Vec<forge_domain::Node>> {
        Ok(Vec::new())
    }

    async fn list_workspaces(&self, _auth_token: &forge_domain::ApiKey) -> Result<Vec<WorkspaceInfo>> {
        Ok(vec![self.workspace.clone()])
    }

    async fn get_workspace(
        &self,
        workspace_id: &WorkspaceId,
        _auth_token: &forge_domain::ApiKey,
    ) -> Result<Option<WorkspaceInfo>> {
        Ok((self.workspace.workspace_id == *workspace_id).then(|| self.workspace.clone()))
    }

    async fn list_workspace_files(
        &self,
        _workspace: &forge_domain::WorkspaceFiles,
        _auth_token: &forge_domain::ApiKey,
    ) -> Result<Vec<FileHash>> {
        Ok(self.remote_files.as_ref().clone())
    }

    async fn delete_files(
        &self,
        _deletion: &forge_domain::FileDeletion,
        _auth_token: &forge_domain::ApiKey,
    ) -> Result<()> {
        Ok(())
    }

    async fn delete_workspace(
        &self,
        _workspace_id: &WorkspaceId,
        _auth_token: &forge_domain::ApiKey,
    ) -> Result<()> {
        Ok(())
    }
}
