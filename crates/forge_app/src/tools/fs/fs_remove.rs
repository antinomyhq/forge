use std::path::Path;
use std::sync::Arc;

use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::utils::assert_absolute_path;
use crate::{FileRemoveService, FsMetaService, Infrastructure};

#[derive(Deserialize, JsonSchema)]
pub struct FSRemoveInput {
    /// The path of the file to remove (absolute path required)
    pub path: String,
}

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
#[derive(ToolDescription)]
pub struct FSRemove<T>(Arc<T>);

impl<T: Infrastructure> FSRemove<T> {
    pub fn new(infra: Arc<T>) -> Self {
        Self(infra)
    }
}

impl<T> NamedTool for FSRemove<T> {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_fs_remove")
    }
}

#[async_trait::async_trait]
impl<T: Infrastructure> ExecutableTool for FSRemove<T> {
    type Input = FSRemoveInput;

    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
        let path = Path::new(&input.path);
        assert_absolute_path(path)?;

        // Check if the file exists
        if !self.0.file_meta_service().exists(path).await? {
            return Err(anyhow::anyhow!("File not found: {}", input.path));
        }

        // Check if it's a file
        if !self.0.file_meta_service().is_file(path).await? {
            return Err(anyhow::anyhow!("Path is not a file: {}", input.path));
        }

        // Remove the file
        self.0.file_remove_service().remove(path).await?;

        Ok(format!("Successfully removed file: {}", input.path))
    }
}

#[cfg(test)]
mod test {
    use bytes::Bytes;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;
    use crate::tools::utils::TempDir;
    use crate::{FsCreateDirsService, FsWriteService};

    #[tokio::test]
    async fn test_fs_remove_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let infra = Arc::new(MockInfrastructure::new());

        // Create a test file
        infra
            .file_write_service()
            .write(
                file_path.as_path(),
                Bytes::from("test content".as_bytes().to_vec()),
            )
            .await
            .unwrap();

        assert!(infra.file_meta_service().exists(&file_path).await.unwrap());

        let fs_remove = FSRemove::new(infra.clone());
        let result = fs_remove
            .call(FSRemoveInput { path: file_path.to_string_lossy().to_string() })
            .await
            .unwrap();

        assert!(result.contains("Successfully removed file"));
        assert!(!infra.file_meta_service().exists(&file_path).await.unwrap());
    }

    #[tokio::test]
    async fn test_fs_remove_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_file = temp_dir.path().join("nonexistent.txt");
        let infra = Arc::new(MockInfrastructure::new());

        let fs_remove = FSRemove::new(infra);
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
        let infra = Arc::new(MockInfrastructure::new());

        // Create a test directory
        infra
            .create_dirs_service()
            .create_dirs(dir_path.as_path())
            .await
            .unwrap();
        assert!(infra
            .file_meta_service()
            .exists(dir_path.as_path())
            .await
            .unwrap());

        let fs_remove = FSRemove::new(infra.clone());
        let result = fs_remove
            .call(FSRemoveInput { path: dir_path.to_string_lossy().to_string() })
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path is not a file"));
        assert!(infra
            .file_meta_service()
            .exists(dir_path.as_path())
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_fs_remove_relative_path() {
        let infra = Arc::new(MockInfrastructure::new());
        let fs_remove = FSRemove::new(infra);
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
