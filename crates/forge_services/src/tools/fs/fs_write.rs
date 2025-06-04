use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use console::strip_ansi_codes;
use forge_display::{DiffFormat, TitleFormat};
// Using FSWriteInput from forge_domain
use forge_domain::ToolOutput;
use forge_domain::{
    ExecutableTool, FSWriteInput, NamedTool, ToolCallContext, ToolDescription, ToolName,
};
use forge_tool_macros::ToolDescription;

use crate::services::EnvironmentService;
use crate::tools::syn;
use crate::utils::{assert_absolute_path, format_display_path};
use crate::{FsMetaService, FsReadService, FsWriteService, Infrastructure};

/// Use it to create a new file at a specified path with the provided content.
/// Always provide absolute paths for file locations. The tool
/// automatically handles the creation of any missing intermediary directories
/// in the specified path.
/// IMPORTANT: DO NOT attempt to use this tool to move or rename files, use the
/// shell tool instead.
#[derive(ToolDescription)]
pub struct FSWrite<F>(Arc<F>);

impl<F: Infrastructure> FSWrite<F> {
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

impl<F> NamedTool for FSWrite<F> {
    fn tool_name() -> ToolName {
        ToolName::new("forge_tool_fs_create")
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for FSWrite<F> {
    type Input = FSWriteInput;

    async fn call(
        &self,
        context: &mut ToolCallContext,
        input: Self::Input,
    ) -> anyhow::Result<ToolOutput> {
        // Validate absolute path requirement
        let path = Path::new(&input.path);
        assert_absolute_path(path)?;

        // Validate file content if it's a supported language file
        let syntax_warning = syn::validate(&input.path, &input.content);

        // Create parent directories if they don't exist
        if let Some(parent) = Path::new(&input.path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directories: {}", input.path))?;
        }

        // Check if the file exists
        let file_exists = self.0.file_meta_service().is_file(path).await?;

        // If file exists and overwrite flag is not set, return an error with the
        // existing content
        if file_exists && !input.overwrite {
            let existing_content = self.0.file_read_service().read_utf8(path).await?;
            return Err(anyhow::anyhow!(
                "File already exists at {}. If you need to overwrite it, set overwrite to true.\n\nExisting content:\n{}",
                input.path,
                existing_content
            ));
        }

        // record the file content before they're modified
        let old_content = if file_exists {
            // if file already exists, we should be able to read it.
            self.0.file_read_service().read_utf8(path).await?
        } else {
            // if file doesn't exist, we should record it as an empty string.
            "".to_string()
        };

        // Write file only after validation passes and directories are created
        self.0
            .file_write_service()
            .write(Path::new(&input.path), Bytes::from(input.content.clone()))
            .await?;

        let mut result = String::new();

        writeln!(result, "---")?;
        writeln!(result, "path: {}", &input.path)?;
        if file_exists {
            writeln!(result, "operation: OVERWRITE")?;
        } else {
            writeln!(result, "operation: CREATE")?;
        }
        writeln!(result, "total_chars: {}", input.content.len())?;
        if let Some(warning) = syntax_warning {
            writeln!(result, "Warning: {}", &warning.to_string())?;
        }
        writeln!(result, "---")?;

        // record the file content after they're modified
        let new_content = self.0.file_read_service().read_utf8(path).await?;
        let diff = DiffFormat::format(&old_content, &new_content);
        let title = if file_exists {
            writeln!(result, "{}", strip_ansi_codes(&diff))?;
            "Overwrite"
        } else {
            "Create"
        };

        // Use the formatted path for display
        let formatted_path = self.format_display_path(path)?;

        context
            .send_text(format!(
                "{}",
                TitleFormat::debug(title).sub_title(formatted_path)
            ))
            .await?;

        context.send_text(diff).await?;

        Ok(ToolOutput::text(result))
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;
    use std::sync::Arc;

    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::attachment::tests::MockInfrastructure;
    use crate::utils::{TempDir, ToolContentExtension};
    use crate::{FsMetaService, FsReadService};

    async fn assert_path_exists(path: impl AsRef<Path>, infra: &MockInfrastructure) {
        assert!(
            infra
                .file_meta_service()
                .is_file(path.as_ref())
                .await
                .is_ok()
                || path.as_ref().exists(),
            "Path should exist"
        );
    }

    #[tokio::test]
    async fn test_fs_write_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Hello, World!";

        let infra = Arc::new(MockInfrastructure::new());
        let fs_write = FSWrite::new(infra.clone());
        let output = fs_write
            .call(
                &mut ToolCallContext::default(),
                FSWriteInput {
                    explanation: None,
                    path: file_path.to_string_lossy().to_string(),
                    content: content.to_string(),
                    overwrite: false,
                },
            )
            .await
            .unwrap()
            .into_string();

        // Normalize the output to remove temp directory paths
        let normalized_output = TempDir::normalize(&output);
        assert_snapshot!(normalized_output);

        // Verify file was actually written
        let content = infra
            .file_read_service()
            .read_utf8(&file_path)
            .await
            .unwrap();
        assert_eq!(content, "Hello, World!")
    }

    #[tokio::test]
    async fn test_fs_write_invalid_rust() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        let infra = Arc::new(MockInfrastructure::new());
        let fs_write = FSWrite::new(infra.clone());
        let result = fs_write
            .call(
                &mut ToolCallContext::default(),
                FSWriteInput {
                    explanation: None,
                    path: file_path.to_string_lossy().to_string(),
                    content: "fn main() { let x = ".to_string(),
                    overwrite: false,
                },
            )
            .await;

        let output = result.unwrap().into_string();
        // Normalize the output to remove temp directory paths
        let normalized_output = TempDir::normalize(&output);
        assert_snapshot!(normalized_output);
    }

    #[tokio::test]
    async fn test_fs_write_valid_rust() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        let infra = Arc::new(MockInfrastructure::new());
        let fs_write = FSWrite::new(infra.clone());
        let content = "fn main() { let x = 42; }";
        let result = fs_write
            .call(
                &mut ToolCallContext::default(),
                FSWriteInput {
                    explanation: None,
                    path: file_path.to_string_lossy().to_string(),
                    content: content.to_string(),
                    overwrite: false,
                },
            )
            .await;

        let output = result.unwrap().into_string();
        // Normalize the output to remove temp directory paths
        let normalized_output = TempDir::normalize(&output);
        assert_snapshot!(normalized_output);
        // Still keep basic assertions for specific conditions
        assert!(!output.contains("Warning:"));

        // Verify file contains valid Rust code
        let content = infra
            .file_read_service()
            .read_utf8(&file_path)
            .await
            .unwrap();
        assert_eq!(content, "fn main() { let x = 42; }");
    }

    #[tokio::test]
    async fn test_fs_write_single_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("new_dir").join("test.txt");
        let content = "Hello from nested file!";

        let infra = Arc::new(MockInfrastructure::new());
        let fs_write = FSWrite::new(infra.clone());
        let result = fs_write
            .call(
                &mut ToolCallContext::default(),
                FSWriteInput {
                    explanation: None,
                    path: nested_path.to_string_lossy().to_string(),
                    content: content.to_string(),
                    overwrite: false,
                },
            )
            .await
            .unwrap();

        // Normalize the output to remove temp directory paths
        let normalized_result = TempDir::normalize(&result.into_string());
        assert_snapshot!(normalized_result);

        // Verify both directory and file were created
        assert_path_exists(&nested_path, &infra).await;
        assert_path_exists(nested_path.parent().unwrap(), &infra).await;

        // Verify content
        let written_content = infra
            .file_read_service()
            .read_utf8(&nested_path)
            .await
            .unwrap();
        assert_eq!(written_content, content);
    }

    #[tokio::test]
    async fn test_fs_write_deep_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let deep_path = temp_dir
            .path()
            .join("level1")
            .join("level2")
            .join("level3")
            .join("deep.txt");
        let content = "Deep in the directory structure";

        let infra = Arc::new(MockInfrastructure::new());
        let fs_write = FSWrite::new(infra.clone());
        let result = fs_write
            .call(
                &mut ToolCallContext::default(),
                FSWriteInput {
                    explanation: None,
                    path: deep_path.to_string_lossy().to_string(),
                    content: content.to_string(),
                    overwrite: false,
                },
            )
            .await
            .unwrap();

        // Normalize the output to remove temp directory paths
        let normalized_result = TempDir::normalize(&result.into_string());
        assert_snapshot!(normalized_result);

        // Verify entire path was created
        assert_path_exists(&deep_path, &infra).await;
        let mut current = deep_path.parent().unwrap();
        while current != temp_dir.path() {
            assert_path_exists(current, &infra).await;
            current = current.parent().unwrap();
        }

        // Verify content
        let written_content = infra
            .file_read_service()
            .read_utf8(&deep_path)
            .await
            .unwrap();
        assert_eq!(written_content, content);
    }

    #[tokio::test]
    async fn test_fs_write_with_different_separators() {
        let temp_dir = TempDir::new().unwrap();

        // Use forward slashes regardless of platform
        let path_str = format!("{}/dir_a/dir_b/file.txt", temp_dir.path().to_string_lossy());
        let content = "Testing path separators";

        let infra = Arc::new(MockInfrastructure::new());
        let fs_write = FSWrite::new(infra.clone());
        let result = fs_write
            .call(
                &mut ToolCallContext::default(),
                FSWriteInput {
                    explanation: None,
                    path: path_str,
                    content: content.to_string(),
                    overwrite: false,
                },
            )
            .await
            .unwrap();

        // Normalize the output to remove temp directory paths
        let normalized_result = TempDir::normalize(&result.into_string());
        assert_snapshot!(normalized_result);

        // Convert to platform path and verify
        let platform_path = Path::new(&temp_dir.path())
            .join("dir_a")
            .join("dir_b")
            .join("file.txt");

        assert_path_exists(&platform_path, &infra).await;

        // Verify content
        let written_content = infra
            .file_read_service()
            .read_utf8(&platform_path)
            .await
            .unwrap();
        assert_eq!(written_content, content);
    }

    #[tokio::test]
    async fn test_fs_write_relative_path() {
        let infra = Arc::new(MockInfrastructure::new());
        let fs_write = FSWrite::new(infra.clone());
        let result = fs_write
            .call(
                &mut ToolCallContext::default(),
                FSWriteInput {
                    explanation: None,
                    path: "relative/path/file.txt".to_string(),
                    content: "test content".to_string(),
                    overwrite: false,
                },
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path must be absolute"));
    }

    #[tokio::test]
    async fn test_fs_write_no_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_overwrite.txt");
        let original_content = "Original content";

        let infra = Arc::new(MockInfrastructure::new());
        // First, create the file
        infra
            .file_write_service()
            .write(&file_path, Bytes::from(original_content))
            .await
            .unwrap();

        // Now attempt to write without overwrite flag
        let fs_write = FSWrite::new(infra.clone());
        let result = fs_write
            .call(
                &mut ToolCallContext::default(),
                FSWriteInput {
                    path: file_path.to_string_lossy().to_string(),
                    content: "New content".to_string(),
                    overwrite: false,
                    explanation: None,
                },
            )
            .await;

        // Should result in an error
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();

        // Error should mention overwrite flag
        assert!(error_msg.contains("set overwrite to true"));

        // Error should contain the original file content
        assert!(error_msg.contains(original_content));

        // Make sure the file wasn't changed
        let content = infra
            .file_read_service()
            .read_utf8(&file_path)
            .await
            .unwrap();
        assert_eq!(content, original_content);
    }

    #[tokio::test]
    async fn test_format_display_path() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create a mock infrastructure with controlled cwd
        let infra = Arc::new(MockInfrastructure::new());
        let fs_write = FSWrite::new(infra);

        // Test with a mock path
        let display_path = fs_write.format_display_path(Path::new(&file_path));

        // Since MockInfrastructure has a fixed cwd of "/test",
        // and our temp path won't start with that, we expect the full path
        assert!(display_path.is_ok());
        assert_eq!(display_path.unwrap(), file_path.display().to_string());
    }

    #[tokio::test]
    async fn test_fs_write_with_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_overwrite.txt");
        let original_content = "Original content";
        let new_content = "New content";

        let infra = Arc::new(MockInfrastructure::new());
        // First, create the file
        infra
            .file_write_service()
            .write(&file_path, Bytes::from(original_content))
            .await
            .unwrap();

        // Now attempt to write with overwrite flag
        let fs_write = FSWrite::new(infra.clone());
        let result = fs_write
            .call(
                &mut ToolCallContext::default(),
                FSWriteInput {
                    path: file_path.to_string_lossy().to_string(),
                    content: new_content.to_string(),
                    overwrite: true,
                    explanation: None,
                },
            )
            .await;

        // Should be successful
        assert!(result.is_ok());
        let success_msg = result.unwrap().into_string();

        // Normalize the output to remove temp directory paths
        let normalized_msg = TempDir::normalize(&success_msg);
        assert_snapshot!(normalized_msg);

        // Verify file was actually overwritten
        let content = infra
            .file_read_service()
            .read_utf8(&file_path)
            .await
            .unwrap();
        assert_eq!(content, new_content);
    }
}
