use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Metadata for a single background process spawned by the shell tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundProcess {
    /// OS process ID.
    pub pid: u32,
    /// The original command string that was executed.
    pub command: String,
    /// Absolute path to the log file capturing stdout/stderr.
    pub log_file: PathBuf,
    /// When the process was spawned.
    pub started_at: DateTime<Utc>,
}

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
#[derive(Debug, Default)]
pub struct BackgroundProcessManager {
    processes: Mutex<Vec<BackgroundProcess>>,
    log_handles: Mutex<Vec<OwnedLogFile>>,
}

impl BackgroundProcessManager {
    /// Creates a new, empty manager.
    pub fn new() -> Self {
        Self::default()
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
            // Simply removing the OwnedLogFile will drop the NamedTempFile,
            // deleting the file on disk.
            self.log_handles
                .lock()
                .expect("lock poisoned")
                .retain(|h| h.pid != pid);
        } else {
            // Persist the file by taking ownership and calling `persist` (or
            // `keep`) so the drop won't delete it.
            let mut handles = self.log_handles.lock().expect("lock poisoned");
            if let Some(pos) = handles.iter().position(|h| h.pid == pid) {
                let owned = handles.remove(pos);
                // Persist: consumes the handle without deleting the file.
                let _ = owned._handle.keep();
            }
        }
    }

    /// Returns the number of tracked processes.
    pub fn len(&self) -> usize {
        self.processes.lock().expect("lock poisoned").len()
    }

    /// Returns true if no background processes are tracked.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
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
        // The temp file should be deleted
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
        // The temp file should be persisted (kept)
        assert!(log_path.exists());

        // Cleanup for test hygiene
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
            // manager dropped here
        }

        // After drop, the temp file should be deleted
        assert!(!log_path.exists());
    }
}
