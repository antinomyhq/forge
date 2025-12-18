//! Database pool error types with actionable resolution hints

use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur during database pool creation
#[derive(Error, Debug)]
pub enum DatabasePoolError {
    /// Insufficient disk space for database operations
    #[error("Insufficient disk space: {available_bytes} bytes available, minimum {required_bytes} bytes required for database at '{path}'")]
    InsufficientDiskSpace {
        path: PathBuf,
        available_bytes: u64,
        required_bytes: u64,
    },

    /// Permission denied for database operation
    #[error("Permission denied: cannot {operation} for '{path}'")]
    PermissionDenied {
        path: PathBuf,
        operation: String,
    },

    /// Database file appears to be locked
    #[error("Database file '{path}' appears to be locked or in use")]
    FileLocked { path: PathBuf },

    /// Failed to create connection pool
    #[error("Failed to create connection pool")]
    PoolCreationFailed {
        database_path: PathBuf,
        #[source]
        source: anyhow::Error,
    },
}

impl DatabasePoolError {
    /// Creates an InsufficientDiskSpace error
    pub fn insufficient_disk_space(
        path: impl Into<PathBuf>,
        available_bytes: u64,
        required_bytes: u64,
    ) -> Self {
        Self::InsufficientDiskSpace {
            path: path.into(),
            available_bytes,
            required_bytes,
        }
    }

    /// Creates a PermissionDenied error
    pub fn permission_denied(path: impl Into<PathBuf>, operation: impl Into<String>) -> Self {
        Self::PermissionDenied {
            path: path.into(),
            operation: operation.into(),
        }
    }

    /// Creates a FileLocked error
    pub fn file_locked(path: impl Into<PathBuf>) -> Self {
        Self::FileLocked { path: path.into() }
    }

    /// Creates a PoolCreationFailed error
    pub fn pool_creation_failed(
        database_path: impl Into<PathBuf>,
        source: impl Into<anyhow::Error>,
    ) -> Self {
        Self::PoolCreationFailed {
            database_path: database_path.into(),
            source: source.into(),
        }
    }

    /// Returns an actionable resolution hint for the error
    pub fn resolution_hint(&self) -> String {
        match self {
            Self::InsufficientDiskSpace {
                path,
                available_bytes,
                required_bytes,
            } => format!(
                "→ Action: Free up disk space or move the database to a different location.\n\
                 → Current: {} MB available\n\
                 → Required: {} MB minimum\n\
                 → Database: {}\n\
                 → Try: df -h",
                available_bytes / 1024 / 1024,
                required_bytes / 1024 / 1024,
                path.display()
            ),
            Self::PermissionDenied { path, operation } => format!(
                "→ Action: Grant appropriate permissions to the file or directory.\n\
                 → Failed operation: {}\n\
                 → Path: {}\n\
                 → Try: chmod +rw '{}' or check parent directory permissions",
                operation,
                path.display(),
                path.display()
            ),
            Self::FileLocked { path } => format!(
                "→ Action: Close other applications using the database.\n\
                 → Try: lsof | grep '{}'",
                path.display()
            ),
            Self::PoolCreationFailed { database_path, .. } => format!(
                "→ Action: Check database file integrity and filesystem compatibility.\n\
                 → Try: sqlite3 '{}' \"PRAGMA integrity_check;\"\n\
                 → Note: Ensure filesystem supports SQLite WAL mode",
                database_path.display()
            ),
        }
    }

    /// Returns a formatted error message with resolution hint
    pub fn with_hint(&self) -> String {
        format!("{}\n\n{}", self, self.resolution_hint())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_insufficient_disk_space_error() {
        let error = DatabasePoolError::insufficient_disk_space(
            "/test/db.sqlite",
            50 * 1024 * 1024,
            100 * 1024 * 1024,
        );

        let hint = error.resolution_hint();
        assert!(hint.contains("50 MB available"));
        assert!(hint.contains("100 MB minimum"));
        assert!(hint.contains("df -h"));
    }

    #[test]
    fn test_permission_denied_error() {
        let error = DatabasePoolError::permission_denied("/test/db.sqlite", "write to file");

        let hint = error.resolution_hint();
        assert!(hint.contains("chmod +rw"));
        assert!(hint.contains("/test/db.sqlite"));
        assert!(hint.contains("write to file"));
    }

    #[test]
    fn test_file_locked_error() {
        let error = DatabasePoolError::file_locked("/test/db.sqlite");

        let hint = error.resolution_hint();
        assert!(hint.contains("lsof"));
        assert!(hint.contains("/test/db.sqlite"));
    }

    #[test]
    fn test_pool_creation_failed_error() {
        let error =
            DatabasePoolError::pool_creation_failed("/test/db.sqlite", anyhow::anyhow!("test"));

        let hint = error.resolution_hint();
        assert!(hint.contains("PRAGMA integrity_check"));
        assert!(hint.contains("/test/db.sqlite"));
    }

    #[test]
    fn test_with_hint_formatting() {
        let error = DatabasePoolError::permission_denied("/test/db.sqlite", "read file");

        let message = error.with_hint();
        assert!(message.contains("Permission denied"));
        assert!(message.contains("→ Action:"));
        assert!(message.contains("chmod +rw"));
    }

    #[test]
    fn test_helper_functions_create_correct_variants() {
        // Test all helper functions create the right variants
        let _ = DatabasePoolError::insufficient_disk_space("/test", 100, 200);
        let _ = DatabasePoolError::permission_denied("/test", "read");
        let _ = DatabasePoolError::file_locked("/test");
        let _ = DatabasePoolError::pool_creation_failed("/test", anyhow::anyhow!("test"));
    }

    #[test]
    fn test_helper_with_string_and_pathbuf() {
        // Test that helpers accept both &str and PathBuf
        let error1 = DatabasePoolError::file_locked("/test/db.sqlite");
        let error2 = DatabasePoolError::file_locked(PathBuf::from("/test/db.sqlite"));

        assert_eq!(
            format!("{}", error1),
            format!("{}", error2),
            "Helper should work with both &str and PathBuf"
        );
    }
}
