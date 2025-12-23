use std::collections::{BTreeSet, HashMap};

use forge_domain::{FileHash, FileNode, FileStatus, SyncStatus};

/// Result of comparing local and server files
///
/// This struct stores local and remote file information and provides methods
/// to compute synchronization operations on-demand. It can derive file statuses
/// and identify which files need to be uploaded, deleted, or modified.
pub struct SyncPlan {
    /// Local files with their content and hashes
    local_files: Vec<FileNode>,
    /// Remote file hashes from the server
    remote_files: Vec<FileHash>,
}

impl SyncPlan {
    /// Creates a sync plan from local files and remote file hashes.
    ///
    /// # Arguments
    ///
    /// * `local_files` - Vector of local files with their content and hashes
    /// * `remote_files` - Vector of remote file hashes from the server
    pub fn new(local_files: Vec<FileNode>, remote_files: Vec<FileHash>) -> Self {
        Self { local_files, remote_files }
    }

    /// Derives file sync statuses by comparing local and remote files.
    ///
    /// # Returns
    ///
    /// A sorted vector of `FileStatus` indicating the sync state of each file:
    /// - `InSync`: File exists in both local and remote with matching hashes
    /// - `Modified`: File exists in both but with different hashes
    /// - `New`: File exists only locally
    /// - `Deleted`: File exists only remotely
    pub fn file_statuses(&self) -> Vec<FileStatus> {
        // Build hash maps for efficient lookup
        let local_hashes: HashMap<&str, &str> = self
            .local_files
            .iter()
            .map(|f| (f.file_path.as_str(), f.hash.as_str()))
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
    /// - `files_to_upload`: Vector of files to upload to remote
    pub fn get_operations(&self) -> (Vec<String>, Vec<forge_domain::FileRead>) {
        let statuses = self.file_statuses();
        let mut files_to_delete = Vec::new();
        let mut files_to_upload = Vec::new();

        // Create a map for quick lookup of local files
        let local_files_map: HashMap<&str, &FileNode> =
            self.local_files.iter().map(|f| (f.file_path.as_str(), f)).collect();

        for status in statuses {
            match status.status {
                SyncStatus::Modified => {
                    files_to_delete.push(status.path.clone());
                    if let Some(file) = local_files_map.get(status.path.as_str()) {
                        files_to_upload.push(forge_domain::FileRead::new(
                            file.file_path.clone(),
                            file.content.clone(),
                        ));
                    }
                }
                SyncStatus::New => {
                    if let Some(file) = local_files_map.get(status.path.as_str()) {
                        files_to_upload.push(forge_domain::FileRead::new(
                            file.file_path.clone(),
                            file.content.clone(),
                        ));
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



#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_file_statuses() {
        let local = vec![
            FileNode {
                file_path: "a.rs".into(),
                content: "content_a".into(),
                hash: "hash_a".into(),
            },
            FileNode {
                file_path: "b.rs".into(),
                content: "modified_content".into(),
                hash: "new_hash".into(),
            },
            FileNode {
                file_path: "d.rs".into(),
                content: "content_d".into(),
                hash: "hash_d".into(),
            },
        ];
        let remote = vec![
            FileHash { path: "a.rs".into(), hash: "hash_a".into() },
            FileHash { path: "b.rs".into(), hash: "old_hash".into() },
            FileHash { path: "c.rs".into(), hash: "hash_c".into() },
        ];

        let plan = SyncPlan::new(local, remote);
        let actual = plan.file_statuses();

        let expected = vec![
            forge_domain::FileStatus::new("a.rs".to_string(), forge_domain::SyncStatus::InSync),
            forge_domain::FileStatus::new("b.rs".to_string(), forge_domain::SyncStatus::Modified),
            forge_domain::FileStatus::new("c.rs".to_string(), forge_domain::SyncStatus::Deleted),
            forge_domain::FileStatus::new("d.rs".to_string(), forge_domain::SyncStatus::New),
        ];

        assert_eq!(actual, expected);
    }
}
