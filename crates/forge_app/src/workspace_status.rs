use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use forge_domain::{FileHash, FileNode, FileStatus, SyncProgress, SyncStatus};

/// Result of comparing local and server files
///
/// This struct stores remote file information and provides methods
/// to compute synchronization operations on-demand. It can derive file statuses
/// and identify which files need to be uploaded, deleted, or modified.
///
/// All paths stored internally are relative to the `base_dir` provided at
/// construction time.
pub struct WorkspaceStatus {
    /// Base directory used to relativize all paths.
    base_dir: PathBuf,
    /// Remote file hashes from the server, with paths relative to `base_dir`.
    remote_files: Vec<FileHash>,
}

impl WorkspaceStatus {
    /// Creates a sync plan from remote file hashes.
    ///
    /// Paths in `remote_files` that are absolute and start with `base_dir` are
    /// stripped to relative paths. Paths that are already relative are kept
    /// as-is.
    ///
    /// # Arguments
    ///
    /// * `base_dir` - The workspace root directory used to relativize paths
    /// * `remote_files` - Vector of remote file hashes from the server
    pub fn new(base_dir: impl Into<PathBuf>, remote_files: Vec<FileHash>) -> Self {
        let base_dir = base_dir.into();
        let remote_files = remote_files
            .into_iter()
            .map(|f| FileHash { path: relativize(&base_dir, &f.path), hash: f.hash })
            .collect();
        Self { base_dir, remote_files }
    }

    /// Derives file sync statuses by comparing local and remote files.
    ///
    /// Paths in `local_files` are relativized against the `base_dir` before
    /// comparison, ensuring consistent relative-path keys in the result.
    ///
    /// # Returns
    ///
    /// A sorted vector of `FileStatus` indicating the sync state of each file:
    /// - `InSync`: File exists in both local and remote with matching hashes
    /// - `Modified`: File exists in both but with different hashes
    /// - `New`: File exists only locally
    /// - `Deleted`: File exists only remotely
    pub fn file_statuses(&self, local_files: Vec<FileHash>) -> Vec<FileStatus> {
        let local_files: Vec<FileHash> = local_files
            .into_iter()
            .map(|f| FileHash { path: relativize(&self.base_dir, &f.path), hash: f.hash })
            .collect();

        // Build hash maps for efficient lookup
        let local_hashes: HashMap<&str, &str> = local_files
            .iter()
            .map(|f| (f.path.as_str(), f.hash.as_str()))
            .collect();
        let remote_hashes: HashMap<&str, &str> = self
            .remote_files
            .iter()
            .map(|f| (f.path.as_str(), f.hash.as_str()))
            .collect();
        // Collect all unique file paths (BTreeSet keeps them sorted)
        let mut all_paths: BTreeSet<&str> = BTreeSet::new();
        all_paths.extend(local_hashes.keys().copied());
        all_paths.extend(remote_hashes.keys().copied());

        // Compute status for each file (already sorted by BTreeSet)
        all_paths
            .into_iter()
            .filter_map(|path| {
                let local_hash = local_hashes.get(path);
                let remote_hash = remote_hashes.get(path);

                let status = match (local_hash, remote_hash) {
                    (Some(l), Some(r)) if l == r => SyncStatus::InSync,
                    (Some(_), Some(_)) => SyncStatus::Modified,
                    (Some(_), None) => SyncStatus::New,
                    (None, Some(_)) => SyncStatus::Deleted,
                    (None, None) => return None, // Skip invalid entries
                };

                Some(FileStatus::new(path.to_string(), status))
            })
            .collect()
    }

    /// Returns the sync operations needed based on file statuses.
    ///
    /// # Returns
    ///
    /// A tuple of (files_to_delete, files_to_upload) where:
    /// - `files_to_delete`: Vector of file paths to delete from remote
    /// - `files_to_upload`: Vector of `FileNode`s to upload to remote
    pub fn get_operations(&self, local_files: Vec<FileNode>) -> (Vec<String>, Vec<FileNode>) {
        // Build a lookup map from relative path to FileNode for resolving uploads.
        let local_map: HashMap<String, FileNode> = local_files
            .into_iter()
            .map(|f| (relativize(&self.base_dir, &f.file_path), f))
            .collect();

        let local_hashes: Vec<FileHash> = local_map
            .values()
            .map(|f| FileHash { path: relativize(&self.base_dir, &f.file_path), hash: f.hash.clone() })
            .collect();

        let statuses = self.file_statuses(local_hashes);
        let mut files_to_delete = Vec::new();
        let mut files_to_upload = Vec::new();

        for status in statuses {
            match status.status {
                SyncStatus::Modified | SyncStatus::New => {
                    if let Some(node) = local_map.get(&status.path).cloned() {
                        files_to_upload.push(node);
                    }
                }
                SyncStatus::Deleted => {
                    files_to_delete.push(status.path);
                }
                SyncStatus::InSync => {
                    // No action needed
                }
            }
        }

        (files_to_delete, files_to_upload)
    }
}

/// Strips `base_dir` from `path` if `path` is absolute and starts with
/// `base_dir`, returning the relative portion as a `String`. If `path` is
/// already relative, or does not start with `base_dir`, it is returned
/// unchanged.
fn relativize(base_dir: &Path, path: &str) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        if let Ok(rel) = p.strip_prefix(base_dir) {
            return rel.to_string_lossy().into_owned();
        }
    }
    path.to_owned()
}

/// Tracks progress of sync operations
pub struct SyncProgressCounter {
    total_files: usize,
    total_operations: usize,
    completed_operation: usize,
}

impl SyncProgressCounter {
    pub fn new(total_files: usize, total_operations: usize) -> Self {
        Self { total_files, total_operations, completed_operation: 0 }
    }

    pub fn complete(&mut self, count: usize) {
        self.completed_operation += count;
    }

    pub fn sync_progress(&self) -> SyncProgress {
        //  2 * total_files >= total_operations >= total_files

        if self.completed_operation >= self.total_operations {
            SyncProgress::Syncing { current: self.total_files, total: self.total_files }
        } else {
            let current: f64 = (self.completed_operation as f64 / self.total_operations as f64)
                * self.total_files as f64;
            SyncProgress::Syncing { current: current.floor() as usize, total: self.total_files }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_file_statuses() {
        let base = "/workspace";
        let local = vec![
            FileHash { path: "/workspace/a.rs".into(), hash: "hash_a".into() },
            FileHash { path: "/workspace/b.rs".into(), hash: "new_hash".into() },
            FileHash { path: "/workspace/d.rs".into(), hash: "hash_d".into() },
        ];
        let remote = vec![
            FileHash { path: "/workspace/a.rs".into(), hash: "hash_a".into() },
            FileHash { path: "/workspace/b.rs".into(), hash: "old_hash".into() },
            FileHash { path: "/workspace/c.rs".into(), hash: "hash_c".into() },
        ];

        let plan = WorkspaceStatus::new(base, remote);
        let actual = plan.file_statuses(local);

        let expected = vec![
            forge_domain::FileStatus::new("a.rs".to_string(), forge_domain::SyncStatus::InSync),
            forge_domain::FileStatus::new("b.rs".to_string(), forge_domain::SyncStatus::Modified),
            forge_domain::FileStatus::new("c.rs".to_string(), forge_domain::SyncStatus::Deleted),
            forge_domain::FileStatus::new("d.rs".to_string(), forge_domain::SyncStatus::New),
        ];

        assert_eq!(actual, expected);
    }

    impl SyncProgressCounter {
        fn next_test(&mut self) -> SyncProgress {
            self.complete(1);
            self.sync_progress()
        }
    }

    #[test]
    fn test_sync_progress_counter() {
        // Assuming 4 files, all need to be deleted and added
        let mut counter = SyncProgressCounter::new(4, 8);

        let actual = counter.sync_progress();
        let expected = SyncProgress::Syncing { current: 0, total: 4 };
        assert_eq!(actual, expected);

        let actual = counter.next_test();
        let expected = SyncProgress::Syncing { current: 0, total: 4 };
        assert_eq!(actual, expected);

        let actual = counter.next_test();
        let expected = SyncProgress::Syncing { current: 1, total: 4 };
        assert_eq!(actual, expected);

        let actual = counter.next_test();
        let expected = SyncProgress::Syncing { current: 1, total: 4 };
        assert_eq!(actual, expected);

        let actual = counter.next_test();
        let expected = SyncProgress::Syncing { current: 2, total: 4 };
        assert_eq!(actual, expected);

        let actual = counter.next_test();
        let expected = SyncProgress::Syncing { current: 2, total: 4 };
        assert_eq!(actual, expected);

        let actual = counter.next_test();
        let expected = SyncProgress::Syncing { current: 3, total: 4 };
        assert_eq!(actual, expected);

        let actual = counter.next_test();
        let expected = SyncProgress::Syncing { current: 3, total: 4 };
        assert_eq!(actual, expected);

        let actual = counter.next_test();
        let expected = SyncProgress::Syncing { current: 4, total: 4 };
        assert_eq!(actual, expected);

        let actual = counter.next_test();
        let expected = SyncProgress::Syncing { current: 4, total: 4 };
        assert_eq!(actual, expected);

        let actual = counter.next_test();
        let expected = SyncProgress::Syncing { current: 4, total: 4 };
        assert_eq!(actual, expected);
    }
}
