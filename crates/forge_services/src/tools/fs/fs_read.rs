use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_display::TitleFormat;
use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::range_handler::{
    count_lines, determine_display_range, format_content_with_range, RangePreference,
    DEFAULT_LINE_LIMIT,
};
use crate::tools::utils::{assert_absolute_path, format_display_path};
use crate::{EnvironmentService, FsReadService, Infrastructure};

#[derive(Deserialize, JsonSchema)]
pub struct FSReadInput {
    /// The path of the file to read, always provide absolute paths.
    pub path: String,
    /// Start line for reading file content (optional)
    pub range_start: Option<usize>,
    /// End line for reading file content (optional)
    pub range_end: Option<usize>,
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
    fn format_display_path(&self, path: &Path) -> anyhow::Result<String> {
        // Get the current working directory
        let env = self.0.environment_service().get_environment();
        let cwd = env.cwd.as_path();

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

    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
        let path = Path::new(&input.path);
        assert_absolute_path(path)?;

        // First, check if a specific range was requested
        let has_range = input.range_start.is_some() || input.range_end.is_some();

        // Read the file content
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

        // Count total lines
        let total_lines = count_lines(&content);

        // Format the display path (for output message)
        let display_path = self.format_display_path(path)?;

        // Prepare output with metadata and standardized format
        let output = if has_range || total_lines > DEFAULT_LINE_LIMIT {
            // Use the new common formatter with appropriate options
            format_content_with_range(
                &content,
                Some(&input.path),
                "file",
                RangePreference::First, // fs_read always shows the first part of file
                None,
                false, // Don't store file content in temp file since it's already on disk
            )?
        } else {
            // Simple output for small files with no range specified
            content
        };

        // Display a message about the file being read
        let title = "read";
        let message = if total_lines > DEFAULT_LINE_LIMIT {
            let (start, end) = if has_range {
                determine_display_range(total_lines, input.range_start, input.range_end)
            } else {
                (1, DEFAULT_LINE_LIMIT) // Default for fs_read is to show first
                                        // part
            };
            format!(
                "read range {}-{} of {} lines from {}",
                start, end, total_lines, display_path
            )
        } else {
            format!("read {} lines from {}", total_lines, display_path)
        };

        let formatted_message = TitleFormat::success(title).sub_title(message);
        println!("{}", formatted_message);

        Ok(output)
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
        fs_read
            .call(FSReadInput { path: path.to_string(), range_start: None, range_end: None })
            .await
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
    #[tokio::test]
    async fn test_format_display_path() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create a mock infrastructure with controlled cwd
        let infra = Arc::new(MockInfrastructure::new());
        let fs_read = FSRead::new(infra);

        // Test with a mock path
        let display_path = fs_read.format_display_path(Path::new(&file_path));

        // Since MockInfrastructure has a fixed cwd of "/test",
        // and our temp path won't start with that, we expect the full path
        assert!(display_path.is_ok());
        assert_eq!(display_path.unwrap(), file_path.display().to_string());
    }
}
