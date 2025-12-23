use std::collections::{HashMap, HashSet};
use std::pin::Pin;

use anyhow::Result;
use forge_domain::{FileHash, FileNode, FileRead};

/// Boxed future type for async closures.
type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Result of comparing local and server files
///
/// This struct is used to plan synchronization operations between local files and
/// remote server state. It identifies which files need to be uploaded, deleted, or
/// modified to bring the server in sync with local state.
pub struct SyncPlan {
    /// Files to delete from server (outdated or orphaned)
    files_to_delete: Vec<String>,
    /// Files to upload (new or changed)
    files_to_upload: Vec<FileRead>,
    /// Files that are modified (exists in both delete and upload)
    modified_files: HashSet<String>,
}

impl SyncPlan {
    /// Creates a sync plan by comparing local files with remote file hashes.
    ///
    /// # Arguments
    ///
    /// * `local_files` - Vector of local files with their content and hashes
    /// * `remote_files` - Vector of remote file hashes from the server
    pub fn new(local_files: Vec<FileNode>, remote_files: Vec<FileHash>) -> Self {
        // Build hash maps for O(1) lookup
        let local_hashes: HashMap<&str, &str> = local_files
            .iter()
            .map(|f| (f.file_path.as_str(), f.hash.as_str()))
            .collect();
        let remote_hashes: HashMap<&str, &str> = remote_files
            .iter()
            .map(|f| (f.path.as_str(), f.hash.as_str()))
            .collect();

        // Files to delete: on server but not local or hash changed
        let files_to_delete: Vec<String> = remote_files
            .iter()
            .filter(|f| local_hashes.get(f.path.as_str()) != Some(&f.hash.as_str()))
            .map(|f| f.path.clone())
            .collect();

        // Files to upload: local files not on server or hash changed
        let files_to_upload: Vec<_> = local_files
            .into_iter()
            .filter(|f| remote_hashes.get(f.file_path.as_str()) != Some(&f.hash.as_str()))
            .map(|f| FileRead::new(f.file_path, f.content))
            .collect();

        // Modified files: paths that appear in both delete and upload lists
        let delete_paths: HashSet<&str> = files_to_delete.iter().map(|s| s.as_str()).collect();
        let modified_files: HashSet<String> = files_to_upload
            .iter()
            .filter(|f| delete_paths.contains(f.path.as_str()))
            .map(|f| f.path.clone())
            .collect();

        Self { files_to_delete, files_to_upload, modified_files }
    }

    /// Returns the total file count. Modified files count as 1 (not 2
    /// operations).
    pub fn total(&self) -> usize {
        self.files_to_delete.len() + self.files_to_upload.len() - self.modified_files.len()
    }

    /// Returns the number of files to delete
    pub fn files_to_delete_count(&self) -> usize {
        self.files_to_delete.len()
    }

    /// Returns the number of files to upload
    pub fn files_to_upload_count(&self) -> usize {
        self.files_to_upload.len()
    }

    /// Returns the number of modified files
    pub fn modified_files_count(&self) -> usize {
        self.modified_files.len()
    }

    /// Returns true if there are no files to sync
    pub fn is_empty(&self) -> bool {
        self.files_to_delete.is_empty()
            && self.files_to_upload.is_empty()
            && self.modified_files.is_empty()
    }

    /// Calculates the score contribution for a batch of paths.
    /// Modified files contribute 0.5 (half for delete, half for upload).
    /// Non-modified files contribute 1.0.
    fn batch_score<'a>(&self, paths: impl Iterator<Item = &'a str>) -> f64 {
        paths
            .map(|path| {
                if self.modified_files.contains(path) {
                    0.5
                } else {
                    1.0
                }
            })
            .sum()
    }

    /// Executes the sync plan in batches, consuming self.
    /// Progress is reported as (current_score, total) where modified files
    /// contribute 0.5 for delete and 0.5 for upload.
    ///
    /// # Arguments
    ///
    /// * `batch_size` - Number of files to process in each batch
    /// * `delete` - Async function to delete a batch of files by path
    /// * `upload` - Async function to upload a batch of files
    /// * `on_progress` - Async callback for progress updates
    ///
    /// # Errors
    ///
    /// Returns an error if any batch operation fails
    pub async fn execute<'a>(
        self,
        batch_size: usize,
        delete: impl Fn(Vec<String>) -> BoxFuture<'a, Result<()>>,
        upload: impl Fn(Vec<FileRead>) -> BoxFuture<'a, Result<()>>,
        on_progress: impl Fn(f64, usize) -> BoxFuture<'a, ()>,
    ) -> Result<()> {
        let total = self.total();
        if total == 0 {
            return Ok(());
        }

        let mut current_score = 0.0;
        on_progress(current_score, total).await;

        // Delete outdated/orphaned files
        for batch in self.files_to_delete.chunks(batch_size) {
            delete(batch.to_vec()).await?;
            current_score += self.batch_score(batch.iter().map(|s| s.as_str()));
            on_progress(current_score, total).await;
        }

        // Upload new/changed files
        for batch in self.files_to_upload.chunks(batch_size) {
            upload(batch.to_vec()).await?;
            current_score += self.batch_score(batch.iter().map(|f| f.path.as_str()));
            on_progress(current_score, total).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn test_sync_plan_new_computes_correct_diff() {
        let local = vec![
            FileNode {
                file_path: "a.rs".into(),
                content: "content_a".into(),
                hash: "hash_a".into(),
            },
            FileNode {
                file_path: "b.rs".into(),
                content: "new_content".into(),
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

        let actual = SyncPlan::new(local, remote);

        // b.rs is modified (in both delete and upload), c.rs is orphaned, d.rs is new
        assert_eq!(actual.files_to_delete, vec!["b.rs", "c.rs"]);
        assert_eq!(
            actual
                .files_to_upload
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            vec!["b.rs", "d.rs"]
        );
        assert!(actual.modified_files.contains("b.rs"));
        assert!(!actual.modified_files.contains("d.rs"));

        // Total should be 3: b.rs (modified = 1), c.rs (deleted = 1), d.rs (new = 1)
        assert_eq!(actual.total(), 3);
    }
}
