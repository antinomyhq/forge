use std::path::Path;

use anyhow::Result;

/// High level project context utilities.
pub struct ProjectContext;

impl ProjectContext {
    /// List all branches in the repository.
    pub fn list_branches(base_path: &Path) -> Result<Vec<String>> {
        let branches_dir = base_path.join(".GCC/branches");
        let mut branches = Vec::new();
        if branches_dir.exists() {
            for entry in std::fs::read_dir(&branches_dir)? {
                let entry = entry?;
                if entry.path().is_dir()
                    && let Some(name) = entry.file_name().to_str()
                {
                    branches.push(name.to_string());
                }
            }
        }
        Ok(branches)
    }

    /// Retrieve the latest commit id for a branch (simple heuristic: highest
    /// filename).
    pub fn latest_commit(base_path: &Path, branch: &str) -> Result<Option<String>> {
        let branch_path = base_path.join(".GCC/branches").join(branch);
        if !branch_path.exists() {
            return Ok(None);
        }
        let mut commits = Vec::new();
        for entry in std::fs::read_dir(&branch_path)? {
            let entry = entry?;
            if entry.path().is_file()
                && let Some(ext) = entry.path().extension()
                && ext == "md"
                && let Some(stem) = entry.path().file_stem()
                && let Some(id) = stem.to_str()
            {
                commits.push(id.to_string());
            }
        }
        if commits.is_empty() {
            return Ok(None);
        }
        commits.sort();
        Ok(commits.last().cloned())
    }
}
