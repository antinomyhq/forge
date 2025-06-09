use std::path::Path;
use std::sync::Arc;

use forge_app::EnvironmentService;
use forge_display::TitleFormat;
use forge_domain::{
    ExecutableTool, FSRemoveInput, NamedTool, ToolCallContext, ToolDescription, ToolName,
    ToolOutput,
};
use forge_tool_macros::ToolDescription;

use crate::utils::{assert_absolute_path, format_display_path};
use crate::{FileRemoveService, FsMetaService, Infrastructure};

// Using FSRemoveInput from forge_domain

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
#[derive(ToolDescription)]
pub struct FSRemove<T>(Arc<T>);

impl<T: Infrastructure> FSRemove<T> {
    pub fn new(infra: Arc<T>) -> Self {
        Self(infra)
    }

    /// Formats a path for display, converting absolute paths to relative when
    /// possible
    ///
    /// If the path starts with the current working directory, returns a
    /// relative path. Otherwise, returns the original absolute path.
    fn format_display_path(&self, path: &Path) -> anyhow::Result<String> {
        // Get the current working directory
        let env = self.0.environment_service().get_environment();
        let cwd = env.cwd.as_path();

        // Use the shared utility function
        format_display_path(path, cwd)
    }

    /// Creates and sends a title for the fs_remove operation
    ///
    /// Sets the title and subtitle for the remove operation, then sends it
    /// via the context channel.
    async fn create_and_send_title(
        &self,
        context: &ToolCallContext,
        path: &Path,
    ) -> anyhow::Result<()> {
        // Format a response with metadata
        let display_path = self.format_display_path(path)?;

        let message = TitleFormat::debug("Remove").sub_title(display_path);

        // Send the formatted message
        context.send_text(message).await?;

        Ok(())
    }
}

impl<T> NamedTool for FSRemove<T> {
    fn tool_name() -> ToolName {
        ToolName::new("forge_tool_fs_remove")
    }
}

#[async_trait::async_trait]
impl<T: Infrastructure> ExecutableTool for FSRemove<T> {
    type Input = FSRemoveInput;

    async fn call(
        &self,
        context: &mut ToolCallContext,
        input: Self::Input,
    ) -> anyhow::Result<ToolOutput> {
        let path = Path::new(&input.path);
        assert_absolute_path(path)?;

        // Check if the file exists
        if !self.0.file_meta_service().exists(path).await? {
            let display_path = self.format_display_path(path)?;
            return Err(anyhow::anyhow!("File not found: {}", display_path));
        }

        // Check if it's a file
        if !self.0.file_meta_service().is_file(path).await? {
            let display_path = self.format_display_path(path)?;
            return Err(anyhow::anyhow!("Path is not a file: {}", display_path));
        }

        // Create and send the title using the extracted method
        self.create_and_send_title(context, path).await?;

        // Remove the file
        self.0.file_remove_service().remove(path).await?;

        // Format the success response with appropriate context
        let display_path = self.format_display_path(path)?;

        Ok(ToolOutput::text(format!(
            "Successfully removed file: {display_path}"
        )))
    }
}

#[cfg(test)]
mod test {
    use bytes::Bytes;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;
    use crate::utils::{TempDir, ToolContentExtension};
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
            .call(
                &mut ToolCallContext::default(),
                FSRemoveInput {
                    path: file_path.to_string_lossy().to_string(),
                    explanation: None,
                },
            )
            .await
            .unwrap();

        // Don't test exact message content as paths may vary
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
            .call(
                &mut ToolCallContext::default(),
                FSRemoveInput {
                    path: nonexistent_file.to_string_lossy().to_string(),
                    explanation: None,
                },
            )
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
            .call(
                &mut ToolCallContext::default(),
                FSRemoveInput {
                    path: dir_path.to_string_lossy().to_string(),
                    explanation: None,
                },
            )
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
            .call(
                &mut ToolCallContext::default(),
                FSRemoveInput { path: "relative/path.txt".to_string(), explanation: None },
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path must be absolute"));
    }
}
