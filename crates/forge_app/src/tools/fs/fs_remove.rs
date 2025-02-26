use std::path::Path;

use anyhow::Context;
use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::utils::assert_absolute_path;
use crate::services::fs::{FileReadService, FileWriteService};

#[derive(Deserialize, JsonSchema)]
pub struct FSRemoveInput {
    /// The path of the file to remove (absolute path required)
    pub path: String,
}

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
#[derive(ToolDescription)]
pub struct FSRemove;

impl NamedTool for FSRemove {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_fs_remove")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for FSRemove {
    type Input = FSRemoveInput;

    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
        let path = Path::new(&input.path);
        assert_absolute_path(path)?;

        // Check if the file exists
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", input.path));
        }

        // Check if it's a file
        if !path.is_file() {
            return Err(anyhow::anyhow!("Path is not a file: {}", input.path));
        }

        // Remove the file
        tokio::fs::remove_file(&input.path)
            .await
            .with_context(|| format!("Failed to remove file {}", input.path))?;

        Ok(format!("Successfully removed file: {}", input.path))
    }
}

#[cfg(test)]
mod test {

    use tokio::fs;

    use super::*;
    use crate::tools::utils::TempDir;

    #[tokio::test]
    async fn test_fs_remove_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create a test file
        FileWriteService::write(&file_path, "test content").await.unwrap();
        assert!(file_path.exists());

        let fs_remove = FSRemove;
        let result = fs_remove
            .call(FSRemoveInput { path: file_path.to_string_lossy().to_string() })
            .await
            .unwrap();

        assert!(result.contains("Successfully removed file"));
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_fs_remove_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_file = temp_dir.path().join("nonexistent.txt");

        let fs_remove = FSRemove;
        let result = fs_remove
            .call(FSRemoveInput { path: nonexistent_file.to_string_lossy().to_string() })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[tokio::test]
    async fn test_fs_remove_directory() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("test_dir");

        // Create a test directory
        fs::create_dir(&dir_path).await.unwrap();
        assert!(dir_path.exists());

        let fs_remove = FSRemove;
        let result = fs_remove
            .call(FSRemoveInput { path: dir_path.to_string_lossy().to_string() })
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path is not a file"));
        assert!(dir_path.exists());
    }

    #[tokio::test]
    async fn test_fs_remove_relative_path() {
        let fs_remove = FSRemove;
        let result = fs_remove
            .call(FSRemoveInput { path: "relative/path.txt".to_string() })
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path must be absolute"));
    }
}
