use std::path::Path;

use anyhow::Context;
use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::utils::assert_absolute_path;

#[derive(Deserialize, JsonSchema)]
pub struct FSFileInfoInput {
    /// The path of the file or directory to inspect (absolute path required)
    pub path: String,
}

/// Request to retrieve detailed metadata about a file or directory at the
/// specified path. Returns comprehensive information including size, creation
/// time, last modified time, permissions, and type. Path must be absolute. Use
/// this when you need to understand file characteristics without reading the
/// actual content.
#[derive(ToolDescription)]
pub struct FSFileInfo;

impl NamedTool for FSFileInfo {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_fs_info")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for FSFileInfo {
    type Input = FSFileInfoInput;

    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
        let path = Path::new(&input.path);
        assert_absolute_path(path)?;

        let meta = tokio::fs::metadata(&input.path)
            .await
            .with_context(|| format!("Failed to get metadata for '{}'", input.path))?;
        Ok(format!("{:?}", meta))
    }
}

#[cfg(test)]
mod test {
    use tokio::fs;

    use super::*;
    use crate::tools::utils::TempDir;

    #[tokio::test]
    async fn test_fs_file_info_on_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content").await.unwrap();

        let fs_info = FSFileInfo;
        let result = fs_info
            .call(FSFileInfoInput { path: file_path.to_string_lossy().to_string() })
            .await
            .unwrap();

        assert!(result.contains("FileType"));
        assert!(result.contains("permissions"));
        assert!(result.contains("modified"));
    }

    #[tokio::test]
    async fn test_fs_file_info_on_directory() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("test_dir");
        fs::create_dir(&dir_path).await.unwrap();

        let fs_info = FSFileInfo;
        let result = fs_info
            .call(FSFileInfoInput { path: dir_path.to_string_lossy().to_string() })
            .await
            .unwrap();

        assert!(result.contains("FileType"));
        assert!(result.contains("permissions"));
        assert!(result.contains("modified"));
    }

    #[tokio::test]
    async fn test_fs_file_info_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_path = temp_dir.path().join("nonexistent");

        let fs_info = FSFileInfo;
        let result = fs_info
            .call(FSFileInfoInput { path: nonexistent_path.to_string_lossy().to_string() })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fs_file_info_relative_path() {
        let fs_info = FSFileInfo;
        let result = fs_info
            .call(FSFileInfoInput { path: "relative/path.txt".to_string() })
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path must be absolute"));
    }
}
