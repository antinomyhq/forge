use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use chrono::Utc;
use forge_domain::BackgroundProcess;
use forge_fs::ForgeFS;

use super::ProcessMetadataService;

/// Owns the temp-file handles for background process log files so that they
/// are automatically cleaned up when the manager is dropped.
struct OwnedLogFile {
    /// Keeping the `NamedTempFile` alive prevents cleanup; when dropped the
    /// file is deleted.
    _handle: tempfile::NamedTempFile,
    /// Associated PID so we can remove the handle when the process is killed.
    pid: u32,
}

impl std::fmt::Debug for OwnedLogFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedLogFile")
            .field("pid", &self.pid)
            .finish()
    }
}

/// Thread-safe registry of background processes spawned during the current
/// session.
///
/// Processes are tracked both in-memory (for the current session) and persisted
/// to disk (for cross-session discovery). When the manager is dropped all owned
/// temp-file handles are released, causing the underlying log files to be
/// deleted automatically.
#[derive(Debug)]
pub struct BackgroundProcessManager {
    processes: Mutex<Vec<BackgroundProcess>>,
    log_handles: Mutex<Vec<OwnedLogFile>>,
    metadata: ProcessMetadataService,
}

impl BackgroundProcessManager {
    /// Creates a new, empty manager that persists process metadata under the
    /// given directory.
    pub fn new(processes_dir: PathBuf) -> Self {
        Self {
            processes: Mutex::new(Vec::new()),
            log_handles: Mutex::new(Vec::new()),
            metadata: ProcessMetadataService::new(processes_dir),
        }
    }

    /// Acquires the processes lock, returning an error if poisoned.
    fn lock_processes(&self) -> Result<std::sync::MutexGuard<'_, Vec<BackgroundProcess>>> {
        self.processes
            .lock()
            .map_err(|e| anyhow::anyhow!("processes lock poisoned: {e}"))
    }

    /// Acquires the log handles lock, returning an error if poisoned.
    fn lock_log_handles(&self) -> Result<std::sync::MutexGuard<'_, Vec<OwnedLogFile>>> {
        self.log_handles
            .lock()
            .map_err(|e| anyhow::anyhow!("log handles lock poisoned: {e}"))
    }

    /// Register a newly spawned background process.
    ///
    /// The process is stored both in-memory and persisted to disk so that
    /// other sessions can discover it.
    ///
    /// # Arguments
    ///
    /// * `pid` - OS process id of the spawned process.
    /// * `command` - The command string that was executed.
    /// * `cwd` - Working directory where the command was spawned.
    /// * `log_file` - Absolute path to the log file.
    /// * `log_handle` - The `NamedTempFile` handle that owns the log file on
    ///   disk. Kept alive until the process is removed or the manager is
    ///   dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal lock is poisoned or disk I/O fails.

    pub async fn register(
        &self,
        pid: u32,
        command: String,
        cwd: PathBuf,
        log_file: PathBuf,
        log_handle: tempfile::NamedTempFile,
    ) -> Result<BackgroundProcess> {
        let process = BackgroundProcess { pid, command, cwd, log_file, started_at: Utc::now() };
        self.metadata.save_process(&process).await?;
        self.lock_processes()?.push(process.clone());
        self.lock_log_handles()?
            .push(OwnedLogFile { _handle: log_handle, pid });
        Ok(process)
    }


    /// Remove a background process by PID.
    ///
    /// This also drops the associated log-file handle. If `delete_log` is
    /// `false` the handle is persisted (leaked) so the file survives on disk.
    /// The process is also removed from disk metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal lock is poisoned or disk I/O fails.
    async fn remove(&self, pid: u32, delete_log: bool) -> Result<()> {
        // Look up the CWD before removing so we can update the disk metadata.
        let cwd = self
            .lock_processes()?
            .iter()
            .find(|p| p.pid == pid)
            .map(|p| p.cwd.clone());

        self.lock_processes()?.retain(|p| p.pid != pid);

        if delete_log {
            self.lock_log_handles()?.retain(|h| h.pid != pid);
        } else {
            let mut handles = self.lock_log_handles()?;
            if let Some(pos) = handles.iter().position(|h| h.pid == pid) {
                let owned = handles.remove(pos);
                let _ = owned._handle.keep();
            }
        }

        if let Some(cwd) = cwd {
            self.metadata.remove_process(&cwd, pid).await?;
        }
        Ok(())
    }

    /// Kills a background process by PID and removes it from tracking.
    ///
    /// Returns `Ok(())` if the process was killed or was already dead.
    /// The `delete_log` flag controls whether the log file is deleted.
    /// The process is also removed from disk metadata only after confirming
    /// the process is no longer alive.
    ///
    /// # Errors
    ///
    /// Returns an error if the process could not be killed or the lock is
    /// poisoned.
    pub async fn kill(&self, pid: u32, delete_log: bool) -> Result<()> {
        kill_process(pid).context("failed to kill background process")?;

        // Give the OS a moment to reap the process, then verify it's gone.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if is_process_alive(pid) {
            anyhow::bail!("process {pid} is still alive after kill signal; metadata preserved");
        }

        self.remove(pid, delete_log).await?;
        Ok(())
    }

    /// Returns all tracked processes with their alive status.
    ///
    /// Merges in-memory processes (current session) with disk-persisted
    /// processes (other sessions). Deduplicates by PID. Dead processes from
    /// crashed sessions are automatically garbage-collected from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal lock is poisoned or disk I/O fails.
    pub async fn list_with_status(&self) -> Result<Vec<(BackgroundProcess, bool)>> {
        // Start with in-memory processes.
        let in_memory: Vec<BackgroundProcess> = self.lock_processes()?.clone();
        let in_memory_pids: std::collections::HashSet<u32> =
            in_memory.iter().map(|p| p.pid).collect();

        // Load persisted processes from disk and merge (skip duplicates).
        let disk_processes = self.metadata.list_all_processes().await?;

        let mut all = in_memory;
        for dp in disk_processes {
            if !in_memory_pids.contains(&dp.pid) {
                all.push(dp);
            }
        }

        // Check alive status and garbage-collect dead disk-only processes.
        let mut result = Vec::with_capacity(all.len());
        for p in &all {
            let alive = is_process_alive(p.pid);
            if !alive && !in_memory_pids.contains(&p.pid) {
                // Dead process from another session -- remove metadata and log.
                self.metadata.remove_process(&p.cwd, p.pid).await.ok();
                ForgeFS::remove_file(&p.log_file).await.ok();
            } else {
                result.push((p.clone(), alive));
            }
        }

        Ok(result)
    }
}

/// Cross-platform check whether a process is still running.
fn is_process_alive(pid: u32) -> bool {
    let s = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::nothing()),
    );
    s.process(sysinfo::Pid::from_u32(pid)).is_some()
}

/// Cross-platform process termination.
///
/// Kills the process and all its descendants by walking the process tree
/// via `sysinfo` and killing every child recursively before killing the
/// root. This ensures that servers spawned as grandchildren (e.g. `nohup
/// npm start` which spawns `node`) are also terminated.
fn kill_process(pid: u32) -> anyhow::Result<()> {
    let s = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::nothing()),
    );

    let target = sysinfo::Pid::from_u32(pid);

    // Collect all descendants (children, grandchildren, etc.) bottom-up.
    let mut to_kill = Vec::new();
    collect_descendants(&s, target, &mut to_kill);
    // Add the root last so children die first.
    to_kill.push(target);

    for pid in &to_kill {
        if let Some(process) = s.process(*pid) {
            process.kill();
        }
    }

    Ok(())
}

/// Recursively collects all descendant PIDs of `parent` into `out`.
fn collect_descendants(
    system: &sysinfo::System,
    parent: sysinfo::Pid,
    out: &mut Vec<sysinfo::Pid>,
) {
    for (child_pid, proc) in system.processes() {
        if proc.parent() == Some(parent) && *child_pid != parent {
            collect_descendants(system, *child_pid, out);
            out.push(*child_pid);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    fn create_temp_log() -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .prefix("forge-bg-test-")
            .suffix(".log")
            .tempfile()
            .unwrap();
        writeln!(f, "test log content").unwrap();
        f
    }

    fn make_manager() -> (BackgroundProcessManager, TempDir) {
        let dir = TempDir::new().unwrap();
        let manager = BackgroundProcessManager::new(dir.path().to_path_buf());
        (manager, dir)
    }

    #[tokio::test]
    async fn test_register_and_list_with_status() {
        let (fixture, _dir) = make_manager();
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        fixture
            .register(
                1234,
                "npm start".to_string(),
                PathBuf::from("/test"),
                log_path.clone(),
                log,
            )
            .await
            .unwrap();

        let actual = fixture.list_with_status().await.unwrap();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].0.pid, 1234);
        assert_eq!(actual[0].0.command, "npm start");
        assert_eq!(actual[0].0.log_file, log_path);
    }

    #[tokio::test]
    async fn test_remove_with_log_deletion() {
        let (fixture, _dir) = make_manager();
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        fixture
            .register(
                100,
                "node app.js".to_string(),
                PathBuf::from("/test"),
                log_path.clone(),
                log,
            )
            .await
            .unwrap();
        assert_eq!(fixture.list_with_status().await.unwrap().len(), 1);

        fixture.remove(100, true).await.unwrap();

        assert_eq!(fixture.list_with_status().await.unwrap().len(), 0);
        assert!(!log_path.exists());
    }

    #[tokio::test]
    async fn test_remove_without_log_deletion() {
        let (fixture, _dir) = make_manager();
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        fixture
            .register(
                200,
                "cargo watch".to_string(),
                PathBuf::from("/test"),
                log_path.clone(),
                log,
            )
            .await
            .unwrap();

        fixture.remove(200, false).await.unwrap();

        assert_eq!(fixture.list_with_status().await.unwrap().len(), 0);
        assert!(log_path.exists());

        let _ = std::fs::remove_file(&log_path);
    }

    #[tokio::test]
    async fn test_multiple_processes() {
        let (fixture, _dir) = make_manager();

        let log1 = create_temp_log();
        let path1 = log1.path().to_path_buf();
        let log2 = create_temp_log();
        let path2 = log2.path().to_path_buf();

        fixture
            .register(
                10,
                "server1".to_string(),
                PathBuf::from("/proj1"),
                path1,
                log1,
            )
            .await
            .unwrap();
        fixture
            .register(
                20,
                "server2".to_string(),
                PathBuf::from("/proj2"),
                path2,
                log2,
            )
            .await
            .unwrap();

        assert_eq!(fixture.list_with_status().await.unwrap().len(), 2);

        fixture.remove(10, true).await.unwrap();

        let actual = fixture.list_with_status().await.unwrap();
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].0.pid, 20);
    }

    #[tokio::test]
    async fn test_drop_cleans_up_temp_files() {
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        {
            let dir = TempDir::new().unwrap();
            let manager = BackgroundProcessManager::new(dir.path().to_path_buf());
            manager
                .register(
                    300,
                    "temp cmd".to_string(),
                    PathBuf::from("/test"),
                    log_path.clone(),
                    log,
                )
                .await
                .unwrap();
            assert!(log_path.exists());
        }

        assert!(!log_path.exists());
    }

    #[tokio::test]
    async fn test_list_with_status_shows_dead_process() {
        let (fixture, _dir) = make_manager();
        let log = create_temp_log();
        let path = log.path().to_path_buf();

        fixture
            .register(
                99999,
                "ghost".to_string(),
                PathBuf::from("/test"),
                path,
                log,
            )
            .await
            .unwrap();

        let actual = fixture.list_with_status().await.unwrap();

        // The process is in-memory (current session), so it's kept even if dead.
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].0.pid, 99999);
        assert!(!actual[0].1);
    }

    #[tokio::test]
    async fn test_register_persists_to_disk() {
        let dir = TempDir::new().unwrap();
        let processes_dir = dir.path().to_path_buf();

        // Register a process in one manager.
        let manager1 = BackgroundProcessManager::new(processes_dir.clone());
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();
        manager1
            .register(
                42,
                "persisted cmd".to_string(),
                PathBuf::from("/project"),
                log_path,
                log,
            )
            .await
            .unwrap();

        // A second manager reading the same directory should see the persisted
        // process (it will be GC'd since PID 42 is dead, so we just verify
        // the metadata service sees it).
        let metadata = ProcessMetadataService::new(processes_dir);
        let persisted = metadata.list_all_processes().await.unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].pid, 42);
        assert_eq!(persisted[0].command, "persisted cmd");
    }

    #[tokio::test]
    async fn test_remove_cleans_up_disk_metadata() {
        let dir = TempDir::new().unwrap();
        let processes_dir = dir.path().to_path_buf();
        let manager = BackgroundProcessManager::new(processes_dir.clone());
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        manager
            .register(
                55,
                "killable".to_string(),
                PathBuf::from("/proj"),
                log_path,
                log,
            )
            .await
            .unwrap();

        // Use remove() directly since kill() requires a real OS process.
        manager.remove(55, true).await.unwrap();

        // Verify removed from disk.
        let metadata = ProcessMetadataService::new(processes_dir);
        let persisted = metadata.list_all_processes().await.unwrap();
        assert!(persisted.is_empty());
    }
}
