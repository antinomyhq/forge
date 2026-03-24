use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;
use forge_domain::BackgroundProcess;

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
/// When the manager is dropped all owned temp-file handles are released,
/// causing the underlying log files to be deleted automatically.
///
/// Optionally persists process metadata to a JSON file so that other processes
/// (e.g. the ZSH plugin) can list and kill background processes that were
/// spawned in earlier invocations.
#[derive(Debug)]
pub struct BackgroundProcessManager {
    processes: Mutex<Vec<BackgroundProcess>>,
    log_handles: Mutex<Vec<OwnedLogFile>>,
    /// Optional path for persisting process metadata to disk.
    persist_path: Option<PathBuf>,
}

impl Default for BackgroundProcessManager {
    fn default() -> Self {
        Self {
            processes: Mutex::new(Vec::new()),
            log_handles: Mutex::new(Vec::new()),
            persist_path: None,
        }
    }
}

impl BackgroundProcessManager {
    /// Creates a new, empty manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a manager that persists process metadata to the given path.
    ///
    /// If the file already exists, previously tracked processes are loaded
    /// (without their log-file handles - those belong to the original session).
    pub fn with_persistence(path: PathBuf) -> Self {
        let processes = Self::load_from_disk(&path).unwrap_or_default();
        Self {
            processes: Mutex::new(processes),
            log_handles: Mutex::new(Vec::new()),
            persist_path: Some(path),
        }
    }

    /// Saves the current process list to the persistence file, if configured.
    fn persist(&self) {
        if let Some(ref path) = self.persist_path {
            let procs = self.processes.lock().expect("lock poisoned");
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, serde_json::to_string_pretty(&*procs).unwrap_or_default());
        }
    }

    /// Loads process list from a JSON file on disk.
    fn load_from_disk(path: &PathBuf) -> Option<Vec<BackgroundProcess>> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Register a newly spawned background process.
    ///
    /// # Arguments
    ///
    /// * `pid` - OS process id of the spawned process.
    /// * `command` - The command string that was executed.
    /// * `log_file` - Absolute path to the log file.
    /// * `log_handle` - The `NamedTempFile` handle that owns the log file on
    ///   disk. Kept alive until the process is removed or the manager is
    ///   dropped.
    pub fn register(
        &self,
        pid: u32,
        command: String,
        log_file: PathBuf,
        log_handle: tempfile::NamedTempFile,
    ) -> BackgroundProcess {
        let process = BackgroundProcess {
            pid,
            command,
            log_file,
            started_at: Utc::now(),
        };
        self.processes
            .lock()
            .expect("lock poisoned")
            .push(process.clone());
        self.log_handles
            .lock()
            .expect("lock poisoned")
            .push(OwnedLogFile { _handle: log_handle, pid });
        self.persist();
        process
    }

    /// Returns a snapshot of all tracked background processes.
    pub fn list(&self) -> Vec<BackgroundProcess> {
        self.processes
            .lock()
            .expect("lock poisoned")
            .clone()
    }

    /// Find a background process by PID.
    pub fn find(&self, pid: u32) -> Option<BackgroundProcess> {
        self.processes
            .lock()
            .expect("lock poisoned")
            .iter()
            .find(|p| p.pid == pid)
            .cloned()
    }

    /// Remove a background process by PID.
    ///
    /// This also drops the associated log-file handle. If `delete_log` is
    /// `false` the handle is persisted (leaked) so the file survives on disk.
    pub fn remove(&self, pid: u32, delete_log: bool) {
        self.processes
            .lock()
            .expect("lock poisoned")
            .retain(|p| p.pid != pid);

        if delete_log {
            self.log_handles
                .lock()
                .expect("lock poisoned")
                .retain(|h| h.pid != pid);
        } else {
            let mut handles = self.log_handles.lock().expect("lock poisoned");
            if let Some(pos) = handles.iter().position(|h| h.pid == pid) {
                let owned = handles.remove(pos);
                let _ = owned._handle.keep();
            }
        }
        self.persist();
    }

    /// Returns the number of tracked processes.
    pub fn len(&self) -> usize {
        self.processes.lock().expect("lock poisoned").len()
    }

    /// Returns true if no background processes are tracked.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Kills a background process by PID and removes it from tracking.
    ///
    /// Returns `Ok(())` if the process was killed or was already dead.
    /// The `delete_log` flag controls whether the log file is deleted.
    pub fn kill(&self, pid: u32, delete_log: bool) -> anyhow::Result<()> {
        kill_process(pid)?;
        self.remove(pid, delete_log);
        Ok(())
    }

    /// Returns a snapshot of all tracked processes with their alive status.
    pub fn list_with_status(&self) -> Vec<(BackgroundProcess, bool)> {
        self.processes
            .lock()
            .expect("lock poisoned")
            .iter()
            .map(|p| {
                let alive = is_process_alive(p.pid);
                (p.clone(), alive)
            })
            .collect()
    }
}

/// Cross-platform check whether a process is still running.
#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use std::process::Command;
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.contains(&pid.to_string())
        })
        .unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
fn is_process_alive(_pid: u32) -> bool {
    false
}

/// Cross-platform process termination.
#[cfg(unix)]
fn kill_process(pid: u32) -> anyhow::Result<()> {
    let ret = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if ret != 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }
        return Err(anyhow::anyhow!("Failed to kill process {pid}: {err}"));
    }
    Ok(())
}

#[cfg(windows)]
fn kill_process(pid: u32) -> anyhow::Result<()> {
    use std::process::Command;
    let output = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("not found") && !stderr.contains("not running") {
            return Err(anyhow::anyhow!(
                "Failed to kill process {pid}: {stderr}"
            ));
        }
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn kill_process(_pid: u32) -> anyhow::Result<()> {
    Err(anyhow::anyhow!(
        "Killing background processes is not supported on this platform"
    ))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use pretty_assertions::assert_eq;

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

    #[test]
    fn test_register_and_list() {
        let fixture = BackgroundProcessManager::new();
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        fixture.register(1234, "npm start".to_string(), log_path.clone(), log);

        let actual = fixture.list();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].pid, 1234);
        assert_eq!(actual[0].command, "npm start");
        assert_eq!(actual[0].log_file, log_path);
    }

    #[test]
    fn test_find_existing() {
        let fixture = BackgroundProcessManager::new();
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        fixture.register(42, "python server.py".to_string(), log_path, log);

        let actual = fixture.find(42);

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().pid, 42);
    }

    #[test]
    fn test_find_missing() {
        let fixture = BackgroundProcessManager::new();

        let actual = fixture.find(999);

        assert!(actual.is_none());
    }

    #[test]
    fn test_remove_with_log_deletion() {
        let fixture = BackgroundProcessManager::new();
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        fixture.register(100, "node app.js".to_string(), log_path.clone(), log);
        assert_eq!(fixture.len(), 1);

        fixture.remove(100, true);

        assert_eq!(fixture.len(), 0);
        assert!(fixture.find(100).is_none());
        assert!(!log_path.exists());
    }

    #[test]
    fn test_remove_without_log_deletion() {
        let fixture = BackgroundProcessManager::new();
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        fixture.register(200, "cargo watch".to_string(), log_path.clone(), log);

        fixture.remove(200, false);

        assert_eq!(fixture.len(), 0);
        assert!(log_path.exists());

        let _ = std::fs::remove_file(&log_path);
    }

    #[test]
    fn test_multiple_processes() {
        let fixture = BackgroundProcessManager::new();

        let log1 = create_temp_log();
        let path1 = log1.path().to_path_buf();
        let log2 = create_temp_log();
        let path2 = log2.path().to_path_buf();

        fixture.register(10, "server1".to_string(), path1, log1);
        fixture.register(20, "server2".to_string(), path2, log2);

        assert_eq!(fixture.len(), 2);
        assert!(fixture.find(10).is_some());
        assert!(fixture.find(20).is_some());

        fixture.remove(10, true);

        assert_eq!(fixture.len(), 1);
        assert!(fixture.find(10).is_none());
        assert!(fixture.find(20).is_some());
    }

    #[test]
    fn test_is_empty() {
        let fixture = BackgroundProcessManager::new();

        assert!(fixture.is_empty());

        let log = create_temp_log();
        let path = log.path().to_path_buf();
        fixture.register(1, "cmd".to_string(), path, log);

        assert!(!fixture.is_empty());
    }

    #[test]
    fn test_drop_cleans_up_temp_files() {
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        {
            let manager = BackgroundProcessManager::new();
            manager.register(300, "temp cmd".to_string(), log_path.clone(), log);
            assert!(log_path.exists());
        }

        assert!(!log_path.exists());
    }

    #[test]
    fn test_persistence_write_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let persist_path = dir.path().join("processes.json");

        {
            let manager = BackgroundProcessManager::with_persistence(persist_path.clone());
            let log = create_temp_log();
            let log_path = log.path().to_path_buf();
            manager.register(500, "persistent cmd".to_string(), log_path, log);
        }

        assert!(persist_path.exists());

        let reloaded = BackgroundProcessManager::with_persistence(persist_path);
        let actual = reloaded.list();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].pid, 500);
        assert_eq!(actual[0].command, "persistent cmd");
    }

    #[test]
    fn test_persistence_removes_entry() {
        let dir = tempfile::tempdir().unwrap();
        let persist_path = dir.path().join("processes.json");

        let manager = BackgroundProcessManager::with_persistence(persist_path.clone());
        let log1 = create_temp_log();
        let log2 = create_temp_log();
        let path1 = log1.path().to_path_buf();
        let path2 = log2.path().to_path_buf();

        manager.register(600, "cmd1".to_string(), path1, log1);
        manager.register(700, "cmd2".to_string(), path2, log2);
        assert_eq!(manager.len(), 2);

        manager.remove(600, true);

        let reloaded = BackgroundProcessManager::with_persistence(persist_path);
        let actual = reloaded.list();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].pid, 700);
    }

    #[test]
    fn test_list_with_status() {
        let fixture = BackgroundProcessManager::new();
        let log = create_temp_log();
        let path = log.path().to_path_buf();

        fixture.register(99999, "ghost".to_string(), path, log);

        let actual = fixture.list_with_status();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].0.pid, 99999);
        assert!(!actual[0].1);
    }
}
