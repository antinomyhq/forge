use std::path::Path;

use anyhow::Result;
use forge_domain::{ContextLevel, GccError, GccResult};

/// High level storage API built on top of the filesystem abstraction.
/// Provides convenient methods used by the executor and services.
pub struct Storage;

impl Storage {
    /// Initialize a GCC repository at the given base path.
    pub fn init(base_path: &Path) -> GccResult<()> {
        crate::gcc::filesystem::init_repository(base_path)
    }

    /// Create a new branch.
    pub fn create_branch(base_path: &Path, name: &str) -> GccResult<()> {
        crate::gcc::filesystem::create_branch(base_path, name)
    }

    /// Write a commit file.
    pub fn write_commit(
        base_path: &Path,
        branch: &str,
        commit_id: &str,
        content: &str,
    ) -> GccResult<()> {
        crate::gcc::filesystem::write_commit(base_path, branch, commit_id, content)
    }

    /// Read the content of a context level (project, branch, or commit).
    pub fn read_context(base_path: &Path, level: &ContextLevel) -> Result<String> {
        let path = crate::gcc::filesystem::context_path(base_path, level);
        std::fs::read_to_string(&path).map_err(|e| GccError::Io(e).into())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_gcc_storage_integration() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path();

        // Test 1: Initialize GCC
        Storage::init(test_path).unwrap();

        // Verify .GCC directory was created
        assert!(test_path.join(".GCC").exists());
        assert!(test_path.join(".GCC/main.md").exists());

        // Test 2: Create main branch
        Storage::create_branch(test_path, "main").unwrap();

        // Verify main branch was created
        assert!(test_path.join(".GCC/branches/main").exists());
        assert!(test_path.join(".GCC/branches/main/log.md").exists());

        // Test 3: Create a commit
        let commit_id = "test-commit-123";
        let commit_message = "Test commit for GCC functionality";
        Storage::write_commit(test_path, "main", commit_id, commit_message).unwrap();

        // Verify commit was created
        let commit_path = test_path.join(format!(".GCC/branches/main/{}.md", commit_id));
        assert!(commit_path.exists());
        let commit_content = fs::read_to_string(&commit_path).unwrap();
        assert!(commit_content.contains("Test commit for GCC functionality"));

        // Test 4: Read context
        let project_context =
            Storage::read_context(test_path, &forge_domain::ContextLevel::Project).unwrap();
        assert!(project_context.contains("GCC Project Overview"));

        let branch_context = Storage::read_context(
            test_path,
            &forge_domain::ContextLevel::Branch("main".to_string()),
        )
        .unwrap();
        assert!(branch_context.contains("Log for branch main"));

        // Test 5: Create another branch
        Storage::create_branch(test_path, "feature-test").unwrap();

        // Verify feature branch was created
        assert!(test_path.join(".GCC/branches/feature-test").exists());
        assert!(test_path.join(".GCC/branches/feature-test/log.md").exists());

        // Test 6: Try to create duplicate branch (should fail)
        let result = Storage::create_branch(test_path, "feature-test");
        assert!(result.is_err());
    }
}
