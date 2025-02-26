use std::path::Path;

use anyhow::Context;
use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::utils::assert_absolute_path;
use crate::services::fs::{FileReadService, FileWriteService};

#[derive(Deserialize, JsonSchema)]
pub struct FSReadInput {
    /// The path of the file to read, always provide absolute paths.
    pub path: String,
}

/// Request to read the contents of a file at the specified path. Use this when
/// you need to examine the contents of an existing file you do not know the
/// contents of, for example to analyze code, review text files, or extract
/// information from configuration files. Automatically extracts raw text from
/// PDF and DOCX files. May not be suitable for other types of binary files, as
/// it returns the raw content as a string.
#[derive(ToolDescription)]
pub struct FSRead;

impl NamedTool for FSRead {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_fs_read")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for FSRead {
    type Input = FSReadInput;

    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
        let path = Path::new(&input.path);
        assert_absolute_path(path)?;

        FileReadService::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read file content from {}", input.path))
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use tokio::fs;

    use super::*;
    use crate::tools::utils::TempDir;

    #[tokio::test]
    async fn test_fs_read_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let test_content = "Hello, World!";
        FileWriteService::write(&file_path, test_content).await.unwrap();

        let fs_read = FSRead;
        let result = fs_read
            .call(FSReadInput { path: file_path.to_string_lossy().to_string() })
            .await
            .unwrap();

        assert_eq!(result, test_content);
    }

    #[tokio::test]
    async fn test_fs_read_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_file = temp_dir.path().join("nonexistent.txt");

        let fs_read = FSRead;
        let result = fs_read
            .call(FSReadInput { path: nonexistent_file.to_string_lossy().to_string() })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fs_read_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");
        FileWriteService::write(&file_path, "").await.unwrap();

        let fs_read = FSRead;
        let result = fs_read
            .call(FSReadInput { path: file_path.to_string_lossy().to_string() })
            .await
            .unwrap();

        assert_eq!(result, "");
    }

    #[test]
    fn test_description() {
        assert!(FSRead.description().len() > 100)
    }

    #[tokio::test]
    async fn test_fs_read_relative_path() {
        let fs_read = FSRead;
        let result = fs_read
            .call(FSReadInput { path: "relative/path.txt".to_string() })
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path must be absolute"));
    }
}
