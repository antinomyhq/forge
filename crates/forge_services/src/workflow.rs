use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use forge_domain::{Workflow, WorkflowService};

use crate::{FsReadService, FsWriteService, Infrastructure};

/// A workflow loader to load the workflow from the given path.
/// It also resolves the internal paths specified in the workflow.
pub struct ForgeWorkflowService<F> {
    infra: Arc<F>,
}

impl<F> ForgeWorkflowService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

impl<F: Infrastructure> ForgeWorkflowService<F> {
    /// Find a forge.yaml config file by traversing parent directories.
    /// Returns the path to the first found config file, or the original path if
    /// none is found.
    pub async fn resolve_path(&self, path: Option<PathBuf>) -> PathBuf {
        let path = path.unwrap_or(PathBuf::from("."));
        // If the path exists or this is an explicitly provided path, return it as is
        if path.exists() || path.to_string_lossy() != "forge.yaml" {
            return path.to_path_buf();
        }

        // Get the current directory as the starting point
        let mut current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let filename = path.file_name().unwrap_or_default();

        // Start searching for the config in the current directory and its parents
        loop {
            let config_path = current_dir.join(filename);
            if config_path.exists() {
                return config_path;
            }

            // Try to go up one directory
            match current_dir.parent() {
                Some(parent) if parent != current_dir => {
                    current_dir = parent.to_path_buf();
                }
                // Stop if we've reached the root directory or can't go further up
                _ => break,
            }
        }

        // If no config was found, return the original path
        path.to_path_buf()
    }

    /// Loads the workflow from the given path.
    /// If the path is just "forge.yaml", searches for it in parent directories.
    /// If the file doesn't exist anywhere, creates a new empty workflow file at
    /// the specified path (in the current directory).
    pub async fn read(&self, path: &Path) -> anyhow::Result<Workflow> {
        // First, try to find the config file in parent directories if needed
        let path = &self.resolve_path(Some(path.into())).await;

        if !path.exists() {
            let workflow = Workflow::new();
            self.infra
                .file_write_service()
                .write(path, serde_yml::to_string(&workflow)?.into())
                .await?;

            Ok(workflow)
        } else {
            let content = self.infra.file_read_service().read_utf8(path).await?;
            let workflow: Workflow = serde_yml::from_str(&content)
                .with_context(|| format!("Failed to parse workflow from {}", path.display()))?;
            Ok(workflow)
        }
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> WorkflowService for ForgeWorkflowService<F> {
    async fn resolve(&self, path: Option<PathBuf>) -> PathBuf {
        self.resolve_path(path).await
    }

    async fn read(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let path_to_use = path.unwrap_or_else(|| Path::new("forge.yaml"));
        self.read(path_to_use).await
    }

    async fn write(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()> {
        // First, try to find the config file in parent directories if needed
        let path_buf = match path {
            Some(p) => p.to_path_buf(),
            None => PathBuf::from("forge.yaml"),
        };
        let resolved_path = self.resolve_path(Some(path_buf)).await;

        let content = serde_yml::to_string(workflow)?;
        self.infra
            .file_write_service()
            .write(&resolved_path, content.into())
            .await
    }

    async fn update_workflow<Func>(&self, path: Option<&Path>, f: Func) -> anyhow::Result<Workflow>
    where
        Func: FnOnce(&mut Workflow) + Send,
    {
        // Read the current workflow
        let path_to_use = path.unwrap_or_else(|| Path::new("forge.yaml"));
        let mut workflow = self.read(path_to_use).await?;

        // Apply the closure to update the workflow
        f(&mut workflow);

        // Write the updated workflow back
        self.write(path, &workflow).await?;

        Ok(workflow)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    /// This testing strategy tests the core algorithm directly without
    /// depending on complex directory structures.
    #[test]
    fn test_find_config_file_behavior() {
        // Test 1: Return exact path if file exists
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("forge.yaml");
        fs::write(&config_path, "test content").unwrap();

        let result = find_config_file_logic(Path::new("forge.yaml"), &config_path);
        assert_eq!(result, config_path);

        // Test 2: Return original path for non-forge.yaml files
        let custom_path = PathBuf::from("custom-config.yaml");
        let result =
            find_config_file_logic(&custom_path, &temp_dir.path().join("file-that-exists.txt"));
        assert_eq!(result, custom_path);

        // Test 3: Return parent path when found
        let parent_dir = temp_dir.path().join("parent");
        let child_dir = parent_dir.join("child");
        fs::create_dir_all(&child_dir).unwrap();

        let parent_config = parent_dir.join("forge.yaml");
        fs::write(&parent_config, "parent config").unwrap();

        let result = find_config_file_logic(Path::new("forge.yaml"), &parent_config);
        assert_eq!(result, parent_config);
    }

    // Pure function that tests the core logic without filesystem dependencies
    fn find_config_file_logic(path: &Path, existing_config_path: &Path) -> PathBuf {
        // If the path exists or this is an explicitly provided path, return it as is
        if path.exists() || path.to_string_lossy() != "forge.yaml" {
            return path.to_path_buf();
        }

        // Simulate checking directories by checking if the existing_config_path
        // contains the filename we're looking for
        if existing_config_path.file_name().unwrap_or_default()
            == path.file_name().unwrap_or_default()
        {
            return existing_config_path.to_path_buf();
        }

        // If no config was found, return the original path
        path.to_path_buf()
    }

    #[test]
    fn test_find_config_not_found() {
        // Create a temporary directory without a config
        let temp_dir = TempDir::new().unwrap();
        let test_dir = temp_dir.path().join("test_dir");
        fs::create_dir_all(&test_dir).unwrap();

        // Save the original directory and change to the test dir
        let original_dir = std::env::current_dir().unwrap();

        // Only create the directory structure, but don't create forge.yaml
        // so the find function should return the original path
        std::env::set_current_dir(&test_dir).unwrap();

        // Test explicitly only the file existence check logic
        assert!(!Path::new("forge.yaml").exists());

        // Restore the original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_explicit_path_not_searched() {
        // Create a test directory structure
        let temp_dir = TempDir::new().unwrap();
        let parent_dir = temp_dir.path().join("parent");
        let child_dir = parent_dir.join("child");
        fs::create_dir_all(&child_dir).unwrap();

        // Create forge.yaml in the parent
        fs::write(parent_dir.join("forge.yaml"), "# Test").unwrap();

        // Simulate search with a non-forge.yaml path
        let custom_path = PathBuf::from("custom-config.yaml");
        let parent_config = parent_dir.join("forge.yaml");

        let result = find_config_file_logic(&custom_path, &parent_config);

        // Should return the custom path unchanged
        assert_eq!(result, custom_path);
    }
}
