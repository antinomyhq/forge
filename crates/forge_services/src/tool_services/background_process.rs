use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
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
#[derive(Default, Debug)]
pub struct BackgroundProcessManager {
    processes: Mutex<Vec<BackgroundProcess>>,
    log_handles: Mutex<Vec<OwnedLogFile>>,
}

impl BackgroundProcessManager {
    /// Creates a new, empty manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquires the processes lock, returning an error if poisoned.
    fn lock_processes(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Vec<BackgroundProcess>>> {
        self.processes
            .lock()
            .map_err(|e| anyhow::anyhow!("processes lock poisoned: {e}"))
    }

    /// Acquires the log handles lock, returning an error if poisoned.
    fn lock_log_handles(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Vec<OwnedLogFile>>> {
        self.log_handles
            .lock()
            .map_err(|e| anyhow::anyhow!("log handles lock poisoned: {e}"))
    }

    /// Register a newly spawned background process.
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
    /// Returns an error if the internal lock is poisoned.
    pub fn register(
        &self,
        pid: u32,
        command: String,
        cwd: PathBuf,
        log_file: PathBuf,
        log_handle: tempfile::NamedTempFile,
    ) -> Result<BackgroundProcess> {
        let process = BackgroundProcess { pid, command, cwd, log_file, started_at: Utc::now() };
        self.lock_processes()?.push(process.clone());
        self.lock_log_handles()?
            .push(OwnedLogFile { _handle: log_handle, pid });
        Ok(process)
    }

    /// Remove a background process by PID.
    ///
    /// This also drops the associated log-file handle. If `delete_log` is
    /// `false` the handle is persisted (leaked) so the file survives on disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal lock is poisoned.
    fn remove(&self, pid: u32, delete_log: bool) -> Result<()> {
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
        Ok(())
    }

    /// Kills a background process by PID and removes it from tracking.
    ///
    /// Returns `Ok(())` if the process was killed or was already dead.
    /// The `delete_log` flag controls whether the log file is deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if the process could not be killed or the lock is
    /// poisoned.
    pub fn kill(&self, pid: u32, delete_log: bool) -> Result<()> {
        kill_process(pid).context("failed to kill background process")?;
        self.remove(pid, delete_log)?;
        Ok(())
    }

    /// Returns a snapshot of all tracked processes with their alive status.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal lock is poisoned.
    pub fn list_with_status(&self) -> Result<Vec<(BackgroundProcess, bool)>> {
        Ok(self
            .lock_processes()?
            .iter()
            .map(|p| {
                let alive = is_process_alive(p.pid);
                (p.clone(), alive)
            })
            .collect())
    }
}

/// Cross-platform check whether a process is still running.
fn is_process_alive(pid: u32) -> bool {
    let s = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing()
            .with_processes(sysinfo::ProcessRefreshKind::nothing()),
    );
    s.process(sysinfo::Pid::from_u32(pid)).is_some()
}

/// Cross-platform process termination.
fn kill_process(pid: u32) -> anyhow::Result<()> {
    let s = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing()
            .with_processes(sysinfo::ProcessRefreshKind::nothing()),
    );
    match s.process(sysinfo::Pid::from_u32(pid)) {
        Some(process) => {
            process.kill();
            Ok(())
        }
        // Process already gone -- nothing to kill.
        None => Ok(()),
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
    fn test_register_and_list_with_status() {
        let fixture = BackgroundProcessManager::new();
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        fixture.register(1234, "npm start".to_string(), PathBuf::from("/test"), log_path.clone(), log).unwrap();

        let actual = fixture.list_with_status().unwrap();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].0.pid, 1234);
        assert_eq!(actual[0].0.command, "npm start");
        assert_eq!(actual[0].0.log_file, log_path);
    }

    #[test]
    fn test_remove_with_log_deletion() {
        let fixture = BackgroundProcessManager::new();
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        fixture.register(100, "node app.js".to_string(), PathBuf::from("/test"), log_path.clone(), log).unwrap();
        assert_eq!(fixture.list_with_status().unwrap().len(), 1);

        fixture.remove(100, true).unwrap();

        assert_eq!(fixture.list_with_status().unwrap().len(), 0);
        assert!(!log_path.exists());
    }

    #[test]
    fn test_remove_without_log_deletion() {
        let fixture = BackgroundProcessManager::new();
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        fixture.register(200, "cargo watch".to_string(), PathBuf::from("/test"), log_path.clone(), log).unwrap();

        fixture.remove(200, false).unwrap();

        assert_eq!(fixture.list_with_status().unwrap().len(), 0);
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

        fixture.register(10, "server1".to_string(), PathBuf::from("/proj1"), path1, log1).unwrap();
        fixture.register(20, "server2".to_string(), PathBuf::from("/proj2"), path2, log2).unwrap();

        assert_eq!(fixture.list_with_status().unwrap().len(), 2);

        fixture.remove(10, true).unwrap();

        let actual = fixture.list_with_status().unwrap();
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].0.pid, 20);
    }

    #[test]
    fn test_drop_cleans_up_temp_files() {
        let log = create_temp_log();
        let log_path = log.path().to_path_buf();

        {
            let manager = BackgroundProcessManager::new();
            manager.register(300, "temp cmd".to_string(), PathBuf::from("/test"), log_path.clone(), log).unwrap();
            assert!(log_path.exists());
        }

        assert!(!log_path.exists());
    }

    #[test]
    fn test_list_with_status_shows_dead_process() {
        let fixture = BackgroundProcessManager::new();
        let log = create_temp_log();
        let path = log.path().to_path_buf();

        fixture.register(99999, "ghost".to_string(), PathBuf::from("/test"), path, log).unwrap();

        let actual = fixture.list_with_status().unwrap();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].0.pid, 99999);
        assert!(!actual[0].1);
    }
}
