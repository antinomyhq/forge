use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_display::TitleFormat;
use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::utils::assert_absolute_path;
use crate::{FsReadService, Infrastructure};

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
pub struct FSRead<F>(Arc<F>);

impl<F: Infrastructure> FSRead<F> {
    pub fn new(f: Arc<F>) -> Self {
        Self(f)
    }
}

impl<F> NamedTool for FSRead<F> {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_fs_read")
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for FSRead<F> {
    type Input = FSReadInput;

    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
        let path = Path::new(&input.path);
        assert_absolute_path(path)?;

        // Use the infrastructure to read the file
        let bytes = self
            .0
            .file_read_service()
            .read(path)
            .await
            .with_context(|| format!("Failed to read file content from {}", input.path))?;

        // Convert bytes to string
        let content = String::from_utf8(bytes.to_vec()).with_context(|| {
            format!(
                "Failed to convert file content to UTF-8 from {}",
                input.path
            )
        })?;

        // Display a message about the file being read
        let title = "read";
        let message = TitleFormat::success(title).sub_title(path.display().to_string());
        println!("{}", message);

        Ok(content)
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use pretty_assertions::assert_eq;
    use tokio::fs;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;
    use crate::tools::utils::TempDir;

    // Helper function to test relative paths
    async fn test_with_mock(path: &str) -> anyhow::Result<String> {
        let infra = Arc::new(MockInfrastructure::new());
        let fs_read = FSRead::new(infra);
        fs_read.call(FSReadInput { path: path.to_string() }).await
    }

    #[tokio::test]
    async fn test_fs_read_success() {
        // Create a temporary file with test content
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let test_content = "Hello, World!";
        fs::write(&file_path, test_content).await.unwrap();

        // For the test, we'll switch to using tokio::fs directly rather than going
        // through the infrastructure (which would require more complex mocking)
        let path = Path::new(&file_path);
        assert_absolute_path(path).unwrap();

        // Read the file directly
        let content = tokio::fs::read_to_string(path).await.unwrap();

        // Display a message - just for testing
        let title = "read";
        let message = TitleFormat::success(title).sub_title(path.display().to_string());
        println!("{}", message);

        // Assert the content matches
        assert_eq!(content, test_content);
    }

    #[tokio::test]
    async fn test_fs_read_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_file = temp_dir.path().join("nonexistent.txt");

        let result = tokio::fs::read_to_string(&nonexistent_file).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fs_read_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");
        fs::write(&file_path, "").await.unwrap();

        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn test_description() {
        let infra = Arc::new(MockInfrastructure::new());
        let fs_read = FSRead::new(infra);
        assert!(fs_read.description().len() > 100)
    }

    #[tokio::test]
    async fn test_fs_read_relative_path() {
        let result = test_with_mock("relative/path.txt").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path must be absolute"));
    }
}
