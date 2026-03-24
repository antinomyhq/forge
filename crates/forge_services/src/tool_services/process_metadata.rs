use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use forge_domain::BackgroundProcess;
use forge_fs::ForgeFS;

/// Handles reading and writing background process metadata JSON files.
///
/// Each CWD gets its own metadata file named `<fnv64_hash_of_cwd>.json`
/// under the configured processes directory. Each file contains a JSON
/// array of `BackgroundProcess` entries.
#[derive(Debug)]
pub struct ProcessMetadataService {
    processes_dir: PathBuf,
}

impl ProcessMetadataService {
    /// Creates a new service that stores metadata under the given directory.
    pub fn new(processes_dir: PathBuf) -> Self {
        Self { processes_dir }
    }

    /// Returns the path to the metadata file for the given CWD.
    fn metadata_path(&self, cwd: &Path) -> PathBuf {
        let hash = BackgroundProcess::cwd_hash(cwd);
        self.processes_dir.join(format!("{hash}.json"))
    }

    /// Persists a background process entry to the metadata file for its CWD.
    ///
    /// If the file already exists, the new entry is appended to the existing
    /// array. Otherwise a new file is created. The parent directory is created
    /// if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns an error if directory creation or file I/O fails.
    pub async fn save_process(&self, process: &BackgroundProcess) -> Result<()> {
        ForgeFS::create_dir_all(&self.processes_dir).await?;

        let path = self.metadata_path(&process.cwd);
        let mut entries = self.read_entries(&path).await;
        entries.push(process.clone());
        self.write_entries(&path, &entries).await
    }

    /// Removes a background process entry by PID from the metadata file for the
    /// given CWD.
    ///
    /// If the array becomes empty after removal the file is deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O fails.
    pub async fn remove_process(&self, cwd: &Path, pid: u32) -> Result<()> {
        let path = self.metadata_path(cwd);
        if !ForgeFS::exists(&path) {
            return Ok(());
        }

        let mut entries = self.read_entries(&path).await;
        entries.retain(|p| p.pid != pid);

        if entries.is_empty() {
            ForgeFS::remove_file(&path).await?;
        } else {
            self.write_entries(&path, &entries).await?;
        }
        Ok(())
    }

    /// Lists all persisted background processes across all CWD metadata files.
    ///
    /// # Errors
    ///
    /// Returns an error if directory reading or file deserialization fails.
    pub async fn list_all_processes(&self) -> Result<Vec<BackgroundProcess>> {
        if !ForgeFS::exists(&self.processes_dir) {
            return Ok(Vec::new());
        }

        let mut all = Vec::new();
        let mut dir = ForgeFS::read_dir(&self.processes_dir).await?;

        while let Some(entry) = dir.next_entry().await? {
            let file_path = entry.path();
            if file_path.extension().is_some_and(|ext| ext == "json") {
                let entries = self.read_entries(&file_path).await;
                all.extend(entries);
            }
        }

        Ok(all)
    }

    /// Reads and deserializes the entries from a metadata file.
    ///
    /// Returns an empty vec if the file doesn't exist or cannot be parsed.
    async fn read_entries(&self, path: &Path) -> Vec<BackgroundProcess> {
        if !ForgeFS::exists(path) {
            return Vec::new();
        }

        match ForgeFS::read_to_string(path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// Serializes and writes entries to a metadata file.
    async fn write_entries(&self, path: &Path, entries: &[BackgroundProcess]) -> Result<()> {
        let json = serde_json::to_string_pretty(entries)
            .context("failed to serialize process metadata")?;
        ForgeFS::write(path, json.as_bytes()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::Utc;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    fn make_process(pid: u32, cwd: &str, command: &str) -> BackgroundProcess {
        BackgroundProcess {
            pid,
            command: command.to_string(),
            cwd: PathBuf::from(cwd),
            log_file: PathBuf::from(format!("/tmp/forge-bg-{pid}.log")),
            started_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_save_and_list_round_trip() {
        let dir = TempDir::new().unwrap();
        let fixture = ProcessMetadataService::new(dir.path().to_path_buf());
        let process = make_process(100, "/a/b/c", "npm start");

        fixture.save_process(&process).await.unwrap();
        let actual = fixture.list_all_processes().await.unwrap();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].pid, 100);
        assert_eq!(actual[0].command, "npm start");
    }

    #[tokio::test]
    async fn test_save_multiple_to_same_cwd() {
        let dir = TempDir::new().unwrap();
        let fixture = ProcessMetadataService::new(dir.path().to_path_buf());
        let p1 = make_process(10, "/proj", "server1");
        let p2 = make_process(20, "/proj", "server2");

        fixture.save_process(&p1).await.unwrap();
        fixture.save_process(&p2).await.unwrap();
        let actual = fixture.list_all_processes().await.unwrap();

        assert_eq!(actual.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_by_pid() {
        let dir = TempDir::new().unwrap();
        let fixture = ProcessMetadataService::new(dir.path().to_path_buf());
        let p1 = make_process(10, "/proj", "server1");
        let p2 = make_process(20, "/proj", "server2");

        fixture.save_process(&p1).await.unwrap();
        fixture.save_process(&p2).await.unwrap();
        fixture
            .remove_process(&PathBuf::from("/proj"), 10)
            .await
            .unwrap();

        let actual = fixture.list_all_processes().await.unwrap();
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].pid, 20);
    }

    #[tokio::test]
    async fn test_remove_last_process_deletes_file() {
        let dir = TempDir::new().unwrap();
        let fixture = ProcessMetadataService::new(dir.path().to_path_buf());
        let process = make_process(100, "/proj", "npm start");
        let meta_path = fixture.metadata_path(&PathBuf::from("/proj"));

        fixture.save_process(&process).await.unwrap();
        assert!(ForgeFS::exists(&meta_path));

        fixture
            .remove_process(&PathBuf::from("/proj"), 100)
            .await
            .unwrap();
        assert!(!ForgeFS::exists(&meta_path));
    }

    #[tokio::test]
    async fn test_list_with_empty_directory() {
        let dir = TempDir::new().unwrap();
        let fixture = ProcessMetadataService::new(dir.path().to_path_buf());

        let actual = fixture.list_all_processes().await.unwrap();

        assert!(actual.is_empty());
    }

    #[tokio::test]
    async fn test_list_with_nonexistent_directory() {
        let fixture = ProcessMetadataService::new(PathBuf::from("/nonexistent/dir"));

        let actual = fixture.list_all_processes().await.unwrap();

        assert!(actual.is_empty());
    }

    #[tokio::test]
    async fn test_list_across_multiple_cwds() {
        let dir = TempDir::new().unwrap();
        let fixture = ProcessMetadataService::new(dir.path().to_path_buf());
        let p1 = make_process(10, "/proj-a", "server");
        let p2 = make_process(20, "/proj-b", "worker");

        fixture.save_process(&p1).await.unwrap();
        fixture.save_process(&p2).await.unwrap();

        let actual = fixture.list_all_processes().await.unwrap();
        assert_eq!(actual.len(), 2);

        let pids: Vec<u32> = actual.iter().map(|p| p.pid).collect();
        assert!(pids.contains(&10));
        assert!(pids.contains(&20));
    }
}
