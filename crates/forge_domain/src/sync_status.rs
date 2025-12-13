use derive_more::Display;
use serde::{Deserialize, Serialize};
use strum_macros::EnumString;

/// Represents the current status of a workspace sync operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
pub enum SyncStatus {
    /// Sync is currently in progress
    #[display("IN_PROGRESS")]
    #[strum(serialize = "IN_PROGRESS")]
    InProgress,
    /// Sync completed successfully
    #[display("SUCCESS")]
    #[strum(serialize = "SUCCESS")]
    Success,
    /// Sync failed with an error
    #[display("FAILED")]
    #[strum(serialize = "FAILED")]
    Failed,
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
    fn test_sync_status_display() {
        let fixture = SyncStatus::Success;
        let actual = format!("{}", fixture);
        let expected = "SUCCESS";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sync_status_display_in_progress() {
        let fixture = SyncStatus::InProgress;
        let actual = format!("{}", fixture);
        let expected = "IN_PROGRESS";
        assert_eq!(actual, expected);
    }
}
