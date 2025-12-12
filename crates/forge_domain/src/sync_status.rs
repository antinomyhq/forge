use std::path::PathBuf;
use std::str::FromStr;

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

impl FromStr for SyncStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "IN_PROGRESS" => Ok(Self::InProgress),
            "SUCCESS" => Ok(Self::Success),
            "FAILED" => Ok(Self::Failed),
            _ => Err(anyhow::anyhow!("Invalid sync status: {}", s)),
        }
    }
}

impl SyncStatus {
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
    /// Timestamp of the last sync state transition (attempt start or
    /// completion)
    pub last_synced_at: DateTime<Utc>,
    /// Error message if the sync failed
    pub error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_sync_status_from_str() {
        let fixture = "IN_PROGRESS";
        let actual: SyncStatus = fixture.parse().unwrap();
        let expected = SyncStatus::InProgress;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sync_status_from_str_success() {
        let fixture = "SUCCESS";
        let actual: SyncStatus = fixture.parse().unwrap();
        let expected = SyncStatus::Success;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sync_status_from_str_failed() {
        let fixture = "FAILED";
        let actual: SyncStatus = fixture.parse().unwrap();
        let expected = SyncStatus::Failed;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sync_status_from_str_invalid() {
        let fixture = "INVALID";
        let actual: Result<SyncStatus, _> = fixture.parse();
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
