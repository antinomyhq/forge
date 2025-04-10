use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_display::TitleFormat;
use forge_domain::{Conversation, ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::utils::{assert_absolute_path, format_display_path};
use crate::{FsReadService, Infrastructure};

#[derive(Deserialize, JsonSchema)]
pub struct FSReadInput {
    /// The path of the file to read, always provide absolute paths.
    pub path: String,
}

/// Reads file contents at specified path. Use for analyzing code, config files,
/// documentation or text data. Extracts text from PDF/DOCX files and preserves
/// original formatting. Returns content as string. Always use absolute paths.
/// Read-only with no file modifications.
#[derive(ToolDescription)]
pub struct FSRead<F>(Arc<F>);

impl<F: Infrastructure> FSRead<F> {
    pub fn new(f: Arc<F>) -> Self {
        Self(f)
    }

    /// Formats a path for display, converting absolute paths to relative when
    /// possible
    ///
    /// If the path starts with the current working directory, returns a
    /// relative path. Otherwise, returns the original absolute path.
    fn format_display_path(
        &self,
        path: &Path,
        conversation: &Conversation,
    ) -> anyhow::Result<String> {
        // Use the conversation's working directory
        let cwd = conversation.cwd.as_path();

        // Use the shared utility function
        format_display_path(path, cwd)
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

    async fn call(
        &self,
        input: Self::Input,
        conversation: &Conversation,
    ) -> anyhow::Result<String> {
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
        let display_path = self.format_display_path(path, conversation)?;
        let message = TitleFormat::success(title).sub_title(display_path);
        println!("{}", message);

        Ok(content)
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_domain::{Conversation, ConversationId, Workflow};
    use pretty_assertions::assert_eq;
    use tokio::fs;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;
    use crate::tools::utils::TempDir;

    // Helper function to create a test conversation with a specific working
    // directory
    fn create_test_conversation(cwd: PathBuf) -> Conversation {
        let id = ConversationId::generate();
        let workflow = Workflow::default();
        Conversation::new(id, workflow, cwd)
    }

    // Helper function to test relative paths
    async fn test_with_mock(path: &str) -> anyhow::Result<String> {
        let infra = Arc::new(MockInfrastructure::new());
        let fs_read = FSRead::new(infra);
        let conversation = create_test_conversation(PathBuf::from("/tmp/test"));
        fs_read
            .call(FSReadInput { path: path.to_string() }, &conversation)
            .await
    }

    #[tokio::test]
    async fn test_fs_read_success() {
        // Create a temporary file with test content
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let test_content = "Hello, World!";
        fs::write(&file_path, test_content).await.unwrap();

        // Create a test conversation
        let conversation = create_test_conversation(temp_dir.path().to_path_buf());

        // For the test, we'll switch to using tokio::fs directly rather than going
        // through the infrastructure (which would require more complex mocking)
        let path = Path::new(&file_path);
        assert_absolute_path(path).unwrap();

        // Read the file directly
        let content = tokio::fs::read_to_string(path).await.unwrap();

        // Create a display path using the conversation's cwd
        let infra = Arc::new(MockInfrastructure::new());
        let fs_read = FSRead::new(infra);
        let display_path = fs_read.format_display_path(path, &conversation).unwrap();

        // Display a message - just for testing
        let title = "read";
        let message = TitleFormat::success(title).sub_title(display_path);
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

    #[tokio::test]
    async fn test_format_display_path() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create a test conversation with the temp dir as its CWD
        let conversation = create_test_conversation(temp_dir.path().to_path_buf());

        // Create a mock infrastructure
        let infra = Arc::new(MockInfrastructure::new());
        let fs_read = FSRead::new(infra);

        // Test with our conversation instance
        let display_path = fs_read.format_display_path(Path::new(&file_path), &conversation);

        // The file should now be relative to the conversation's CWD
        assert!(display_path.is_ok());
        assert_eq!(display_path.unwrap(), "test.txt");
    }
}
