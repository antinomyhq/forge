use forge_domain::SyncProgress;

/// Extensions for formatting `SyncProgress` events as human-readable strings.
///
/// This module contains display logic for sync operation events, converting
/// them into user-friendly messages for the UI layer.
pub trait SyncProgressDisplay {
    /// Returns a human-readable status message for this event.
    ///
    /// Returns `None` for internal events that don't need user-facing messages.
    fn message(&self) -> Option<String>;
}

impl SyncProgressDisplay for SyncProgress {
    fn message(&self) -> Option<String> {
        match self {
            Self::Starting => Some("Initializing sync".to_string()),
            Self::WorkspaceCreated { workspace_id } => {
                Some(format!("Created Workspace: {}", workspace_id))
            }
            Self::DiscoveringFiles { path: _ } => None,
            Self::FilesDiscovered { count: _ } => None,
            Self::ComparingFiles { .. } => None,
            Self::DiffComputed { added, deleted, modified } => {
                let total = added + deleted + modified;
                if total == 0 {
                    Some("Index is up to date".to_string())
                } else {
                    let mut parts = Vec::new();
                    if *added > 0 {
                        parts.push(format!("{} added", added));
                    }
                    if *modified > 0 {
                        parts.push(format!("{} modified", modified));
                    }
                    if *deleted > 0 {
                        parts.push(format!("{} removed", deleted));
                    }
                    Some(format!("Change scan completed [{}]", parts.join(", ")))
                }
            }
            Self::Syncing { current, total } => {
                let width = total.to_string().len();
                let file_word = pluralize(*total);
                Some(format!(
                    "Syncing {:>width$}/{} {}",
                    current, total, file_word
                ))
            }
            Self::Completed { uploaded_files, total_files, failed_files } => {
                if *uploaded_files == 0 && *failed_files == 0 {
                    Some(format!(
                        "Index up to date [{} {}]",
                        total_files,
                        pluralize(*total_files)
                    ))
                } else if *failed_files == 0 {
                    Some(format!(
                        "Sync completed successfully [{uploaded_files}/{total_files} {} updated]",
                        pluralize(*uploaded_files),
                    ))
                } else {
                    Some(format!(
                        "Sync completed with errors [{uploaded_files}/{total_files} {} updated, {failed_files} failed]",
                        pluralize(*uploaded_files),
                    ))
                }
            }
        }
    }
}

/// Returns "file" or "files" based on count.
fn pluralize(count: usize) -> &'static str {
    if count == 1 { "file" } else { "files" }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_starting_message() {
        let fixture = SyncProgress::Starting;
        let actual = fixture.message();
        let expected = Some("Initializing sync".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_diff_computed_no_changes() {
        let fixture = SyncProgress::DiffComputed { added: 0, deleted: 0, modified: 0 };
        let actual = fixture.message();
        let expected = Some("Index is up to date".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_diff_computed_with_changes() {
        let fixture = SyncProgress::DiffComputed { added: 3, deleted: 1, modified: 2 };
        let actual = fixture.message();
        let expected = Some("Change scan completed [3 added, 2 modified, 1 removed]".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syncing_single_file() {
        let fixture = SyncProgress::Syncing { current: 1, total: 1 };
        let actual = fixture.message();
        let expected = Some("Syncing 1/1 file".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_syncing_multiple_files() {
        let fixture = SyncProgress::Syncing { current: 5, total: 10 };
        let actual = fixture.message();
        let expected = Some("Syncing  5/10 files".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_completed_no_uploads() {
        let fixture =
            SyncProgress::Completed { uploaded_files: 0, total_files: 100, failed_files: 0 };
        let actual = fixture.message();
        let expected = Some("Index up to date [100 files]".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_completed_with_uploads() {
        let fixture =
            SyncProgress::Completed { uploaded_files: 5, total_files: 100, failed_files: 0 };
        let actual = fixture.message();
        let expected = Some("Sync completed successfully [5/100 files updated]".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_completed_with_failures() {
        let fixture =
            SyncProgress::Completed { uploaded_files: 5, total_files: 100, failed_files: 3 };
        let actual = fixture.message();
        let expected =
            Some("Sync completed with errors [5/100 files updated, 3 failed]".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_discovering_files_returns_none() {
        let fixture =
            SyncProgress::DiscoveringFiles { path: std::path::PathBuf::from("/some/path") };
        let actual = fixture.message();
        let expected = None;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_pluralize() {
        assert_eq!(pluralize(0), "files");
        assert_eq!(pluralize(1), "file");
        assert_eq!(pluralize(2), "files");
        assert_eq!(pluralize(100), "files");
    }
}
