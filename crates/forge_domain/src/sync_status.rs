use std::path::PathBuf;

use chrono::{DateTime, Utc};
use derive_more::Display;
use serde::{Deserialize, Serialize};

/// Represents the current status of a workspace sync operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum SyncStatus {
    /// Sync is currently in progress
    #[display("IN_PROGRESS")]
    InProgress,
    /// Sync completed successfully
    #[display("SUCCESS")]
    Success,
    /// Sync failed with an error
    #[display("FAILED")]
    Failed,
}

impl SyncStatus {
    /// Parse a sync status from a string representation
    ///
    /// # Errors
    /// Returns an error if the string is not a valid sync status
    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "IN_PROGRESS" => Ok(Self::InProgress),
            "SUCCESS" => Ok(Self::Success),
            "FAILED" => Ok(Self::Failed),
            _ => Err(anyhow::anyhow!("Invalid sync status: {}", s)),
        }
    }

    /// Convert the sync status to its string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InProgress => "IN_PROGRESS",
            Self::Success => "SUCCESS",
            Self::Failed => "FAILED",
        }
    }
}

/// Domain entity representing workspace sync status
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceSyncStatus {
    /// Canonical path to the workspace
    pub path: PathBuf,
    /// Current sync status
    pub status: SyncStatus,
    /// Timestamp of the last sync attempt
    pub last_synced_at: DateTime<Utc>,
    /// Error message if the sync failed
    pub error_message: Option<String>,
    /// Process ID that initiated the sync
    pub process_id: u32,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_sync_status_from_str() {
        let fixture = "IN_PROGRESS";
        let actual = SyncStatus::from_str(fixture).unwrap();
        let expected = SyncStatus::InProgress;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sync_status_from_str_success() {
        let fixture = "SUCCESS";
        let actual = SyncStatus::from_str(fixture).unwrap();
        let expected = SyncStatus::Success;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sync_status_from_str_failed() {
        let fixture = "FAILED";
        let actual = SyncStatus::from_str(fixture).unwrap();
        let expected = SyncStatus::Failed;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sync_status_from_str_invalid() {
        let fixture = "INVALID";
        let actual = SyncStatus::from_str(fixture);
        assert!(actual.is_err());
    }

    #[test]
    fn test_sync_status_as_str() {
        let fixture = SyncStatus::InProgress;
        let actual = fixture.as_str();
        let expected = "IN_PROGRESS";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sync_status_display() {
        let fixture = SyncStatus::Success;
        let actual = format!("{}", fixture);
        let expected = "SUCCESS";
        assert_eq!(actual, expected);
    }
}
