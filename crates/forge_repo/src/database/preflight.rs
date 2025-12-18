//! Preflight checks for database connection setup
//!
//! Validates system conditions before attempting to create database
//! connections, providing actionable error messages for common failure
//! scenarios.

use std::path::Path;

use sysinfo::{Disks, System};
use tracing::debug;

use super::pool_error::DatabasePoolError;

/// Minimum required free space in bytes (100 MB)
const MIN_FREE_SPACE_BYTES: u64 = 100 * 1024 * 1024;

/// Result of preflight checks
pub type Result<T> = std::result::Result<T, DatabasePoolError>;

/// Runs all preflight checks before creating a database connection pool
///
/// # Errors
///
/// Returns specific typed errors for each failure scenario:
/// - `InsufficientDiskSpace`: Not enough free disk space
/// - `PermissionDenied`: Missing file or directory permissions
/// - `FileLocked`: Database file is locked by another process
/// - `PoolCreationFailed`: Other preflight validation failures
pub fn run_preflight_checks(db_path: &Path) -> Result<()> {
    debug!(database_path = %db_path.display(), "Running preflight checks");

    // Ensure parent directory exists and is writable
    ensure_parent_directory_accessible(db_path)?;

    // Check available disk space
    check_disk_space(db_path)?;

    // If database file exists, verify it's accessible
    if db_path.exists() {
        check_file_accessible(db_path)?;
        check_wal_files_accessible(db_path)?;
        check_file_not_locked(db_path)?;
    }

    debug!("Preflight checks completed successfully");
    Ok(())
}

/// Ensures the parent directory exists and is writable
fn ensure_parent_directory_accessible(db_path: &Path) -> Result<()> {
    let parent = db_path
        .parent()
        .ok_or_else(|| DatabasePoolError::permission_denied(db_path, "access parent directory"))?;

    // Create parent directory if it doesn't exist
    if !parent.exists() {
        std::fs::create_dir_all(parent).map_err(|source| {
            DatabasePoolError::permission_denied(parent, format!("create directory: {source}"))
        })?;
        debug!(directory = %parent.display(), "Created parent directory");
    }

    // Verify parent directory is writable
    if parent
        .metadata()
        .map_err(|source| {
            DatabasePoolError::permission_denied(
                parent,
                format!("read directory metadata: {source}"),
            )
        })?
        .permissions()
        .readonly()
    {
        return Err(DatabasePoolError::permission_denied(
            parent,
            "write to directory (read-only)",
        ));
    }

    Ok(())
}

/// Checks if sufficient disk space is available
fn check_disk_space(db_path: &Path) -> Result<()> {
    let available_bytes = get_available_disk_space(db_path)?;

    if available_bytes < MIN_FREE_SPACE_BYTES {
        return Err(DatabasePoolError::insufficient_disk_space(
            db_path,
            available_bytes,
            MIN_FREE_SPACE_BYTES,
        ));
    }

    debug!(
        available_mb = available_bytes / (1024 * 1024),
        required_mb = MIN_FREE_SPACE_BYTES / (1024 * 1024),
        "Disk space check passed"
    );

    Ok(())
}

/// Verifies the database file is readable and writable
fn check_file_accessible(db_path: &Path) -> Result<()> {
    let metadata = db_path.metadata().map_err(|source| {
        DatabasePoolError::permission_denied(db_path, format!("read file metadata: {source}"))
    })?;

    // Check if file is read-only
    if metadata.permissions().readonly() {
        return Err(DatabasePoolError::permission_denied(
            db_path,
            "write to file (read-only)",
        ));
    }

    // Try to open for reading and writing
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(db_path)
        .map_err(|source| {
            DatabasePoolError::permission_denied(
                db_path,
                format!("open file for reading/writing: {source}"),
            )
        })?;

    debug!(file = %db_path.display(), "File accessibility check passed");
    Ok(())
}

/// Checks accessibility of WAL (Write-Ahead Log) files
fn check_wal_files_accessible(db_path: &Path) -> Result<()> {
    let wal_path = db_path.with_extension("db-wal");
    let shm_path = db_path.with_extension("db-shm");

    // WAL files might not exist yet, which is fine
    for wal_file in &[wal_path, shm_path] {
        if wal_file.exists() {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(wal_file)
                .map_err(|source| {
                    DatabasePoolError::permission_denied(
                        wal_file,
                        format!("access WAL file: {source}"),
                    )
                })?;

            debug!(wal_file = %wal_file.display(), "WAL file accessibility check passed");
        }
    }

    Ok(())
}

/// Gets available disk space for the given path
fn get_available_disk_space(path: &Path) -> Result<u64> {
    let disks = Disks::new_with_refreshed_list();
    let path_str = path.to_string_lossy();

    // Find the disk that contains this path
    for disk in disks.list() {
        let mount_point = disk.mount_point().to_string_lossy();
        if path_str.starts_with(mount_point.as_ref()) {
            return Ok(disk.available_space());
        }
    }

    // Fallback: try to get system info
    let mut sys = System::new();
    sys.refresh_all();

    Err(DatabasePoolError::pool_creation_failed(
        path,
        anyhow::anyhow!("Could not determine disk space for path"),
    ))
}

/// Checks if a file is currently locked by another process
fn check_file_not_locked(path: &Path) -> Result<()> {
    // Attempt to open the file in read-write mode
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| DatabasePoolError::file_locked(path))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn test_check_disk_space_sufficient() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Should pass if there's sufficient disk space (most systems will have > 100
        // MB)
        let result = check_disk_space(&db_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_ensure_parent_directory_accessible_creates_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nested_path = temp_dir.path().join("nested/deep/test.db");

        let result = ensure_parent_directory_accessible(&nested_path);
        assert!(result.is_ok());
        assert!(nested_path.parent().unwrap().exists());
    }

    #[test]
    fn test_check_file_accessible_nonexistent_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("nonexistent.db");

        let result = check_file_accessible(&db_path);
        assert!(matches!(
            result,
            Err(DatabasePoolError::PermissionDenied { .. })
        ));
    }

    #[test]
    fn test_check_file_accessible_readonly_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("readonly.db");

        // Create a file and make it read-only
        fs::write(&db_path, b"test").unwrap();
        let mut perms = fs::metadata(&db_path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&db_path, perms).unwrap();

        let result = check_file_accessible(&db_path);
        assert!(matches!(
            result,
            Err(DatabasePoolError::PermissionDenied { .. })
        ));
    }
}
