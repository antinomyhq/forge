use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_display::DiffFormat;
use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::syn;
use crate::tools::utils::assert_absolute_path;
use crate::{FileReadService, FileWriteService, Infrastructure};

#[derive(Deserialize, JsonSchema)]
pub struct FSWriteInput {
    /// The path of the file to write to (absolute path required)
    pub path: String,
    /// The content to write to the file. ALWAYS provide the COMPLETE intended
    /// content of the file, without any truncation or omissions. You MUST
    /// include ALL parts of the file, even if they haven't been modified.
    pub content: String,
    /// If set to true, existing files will be overwritten. If not set and the
    /// file exists, an error will be returned with the content of the
    /// existing file.
    #[serde(default)]
    pub overwrite: bool,
}

pub struct FSWrite<F: Infrastructure>(Arc<F>);

impl<F: Infrastructure> ToolDescription for FSWrite<F> {
    fn description(&self) -> String {
        "Use it to create a new file at a specified path with the provided content.
        Always provide absolute paths for file locations. The tool
        automatically handles the creation of any missing intermediary directories
        in the specified path.
        IMPORTANT: DO NOT attempt to use this tool to move or rename files, use the
        shell tool instead."
            .to_string()
    }
}

impl<F: Infrastructure> FSWrite<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

impl<F: Infrastructure> NamedTool for FSWrite<F> {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_fs_create")
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for FSWrite<F> {
    type Input = FSWriteInput;

    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
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
        let file_exists = path.is_file();

        // If file exists and overwrite flag is not set, return an error with the
        // existing content
        if file_exists && !input.overwrite {
            let existing_content = tokio::fs::read_to_string(path).await?;
            return Err(anyhow::anyhow!(
                "File already exists at {}. If you need to overwrite it, set overwrite to true.\n\nExisting content:\n{}",
                input.path,
                existing_content
            ));
        }

        // record the file content before they're modified
        let old_content = if file_exists {
            // if file already exists, we should be able to read it.
            String::from_utf8_lossy(&self.0.file_read_service().read(path).await?).to_string()
        } else {
            // if file doesn't exist, we should record it as an empty string.
            "".to_string()
        };

        // Write file only after validation passes and directories are created
        self.0
            .file_write_service()
            .write(path, &input.content)
            .await?;

        let mut result = format!(
            "Successfully wrote {} bytes to {}",
            input.content.len(),
            input.path
        );
        if let Some(warning) = syntax_warning {
            result.push_str("\nWarning: ");
            result.push_str(&warning.to_string());
        }

        // record the file content after they're modified
        let new_content = String::from_utf8_lossy(&self.0.file_read_service().read(path).await?.to_vec()).to_string();
        let title = if file_exists { "overwrite" } else { "create" };
        let diff = DiffFormat::format(title, path.to_path_buf(), &old_content, &new_content);
        println!("{}", diff);

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use pretty_assertions::assert_eq;
    use tokio::fs;

    use super::*;
    use crate::tools::tests::Stub;
    use crate::tools::utils::TempDir;

    async fn assert_path_exists(path: impl AsRef<Path>) {
        assert!(fs::metadata(path).await.is_ok(), "Path should exist");
    }

    #[tokio::test]
    async fn test_fs_write_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Hello, World!";

        let fs_write = FSWrite::new(Arc::new(Stub::default()));
        let output = fs_write
            .call(FSWriteInput {
                path: file_path.to_string_lossy().to_string(),
                content: content.to_string(),
                overwrite: false,
            })
            .await
            .unwrap();

        assert!(output.contains("Successfully wrote"));
        assert!(output.contains(&file_path.display().to_string()));
        assert!(output.contains(&content.len().to_string()));

        // Verify file was actually written
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "Hello, World!")
    }

    #[tokio::test]
    async fn test_fs_write_invalid_rust() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        let fs_write = FSWrite::new(Arc::new(Stub::default()));
        let result = fs_write
            .call(FSWriteInput {
                path: file_path.to_string_lossy().to_string(),
                content: "fn main() { let x = ".to_string(),
                overwrite: false,
            })
            .await;

        let output = result.unwrap();
        assert!(output.contains("Warning:"));
    }

    #[tokio::test]
    async fn test_fs_write_valid_rust() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        let fs_write = FSWrite::new(Arc::new(Stub::default()));
        let content = "fn main() { let x = 42; }";
        let result = fs_write
            .call(FSWriteInput {
                path: file_path.to_string_lossy().to_string(),
                content: content.to_string(),
                overwrite: false,
            })
            .await;

        let output = result.unwrap();
        assert!(output.contains("Successfully wrote"));
        assert!(output.contains(&file_path.display().to_string()));
        assert!(output.contains(&content.len().to_string()));
        assert!(!output.contains("Warning:"));
        // Verify file contains valid Rust code
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "fn main() { let x = 42; }");
    }

    #[tokio::test]
    async fn test_fs_write_single_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("new_dir").join("test.txt");
        let content = "Hello from nested file!";

        let fs_write = FSWrite::new(Arc::new(Stub::default()));
        let result = fs_write
            .call(FSWriteInput {
                path: nested_path.to_string_lossy().to_string(),
                content: content.to_string(),
                overwrite: false,
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully wrote"));
        // Verify both directory and file were created
        assert_path_exists(&nested_path).await;
        assert_path_exists(nested_path.parent().unwrap()).await;

        // Verify content
        let written_content = fs::read_to_string(&nested_path).await.unwrap();
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

        let fs_write = FSWrite::new(Arc::new(Stub::default()));
        let result = fs_write
            .call(FSWriteInput {
                path: deep_path.to_string_lossy().to_string(),
                content: content.to_string(),
                overwrite: false,
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully wrote"));

        // Verify entire path was created
        assert_path_exists(&deep_path).await;
        let mut current = deep_path.parent().unwrap();
        while current != temp_dir.path() {
            assert_path_exists(current).await;
            current = current.parent().unwrap();
        }

        // Verify content
        let written_content = fs::read_to_string(&deep_path).await.unwrap();
        assert_eq!(written_content, content);
    }

    #[tokio::test]
    async fn test_fs_write_with_different_separators() {
        let temp_dir = TempDir::new().unwrap();

        // Use forward slashes regardless of platform
        let path_str = format!("{}/dir_a/dir_b/file.txt", temp_dir.path().to_string_lossy());
        let content = "Testing path separators";

        let fs_write = FSWrite::new(Arc::new(Stub::default()));
        let result = fs_write
            .call(FSWriteInput {
                path: path_str,
                content: content.to_string(),
                overwrite: false,
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully wrote"));

        // Convert to platform path and verify
        let platform_path = Path::new(&temp_dir.path())
            .join("dir_a")
            .join("dir_b")
            .join("file.txt");

        assert_path_exists(&platform_path).await;

        // Verify content
        let written_content = fs::read_to_string(&platform_path).await.unwrap();
        assert_eq!(written_content, content);
    }

    #[tokio::test]
    async fn test_fs_write_relative_path() {
        let fs_write = FSWrite::new(Arc::new(Stub::default()));
        let result = fs_write
            .call(FSWriteInput {
                path: "relative/path/file.txt".to_string(),
                content: "test content".to_string(),
                overwrite: false,
            })
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

        // First, create the file
        fs::write(&file_path, original_content).await.unwrap();

        // Now attempt to write without overwrite flag
        let fs_write = FSWrite::new(Arc::new(Stub::default()));
        let result = fs_write
            .call(FSWriteInput {
                path: file_path.to_string_lossy().to_string(),
                content: "New content".to_string(),
                overwrite: false,
            })
            .await;

        // Should result in an error
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();

        // Error should mention overwrite flag
        assert!(error_msg.contains("set overwrite to true"));

        // Error should contain the original file content
        assert!(error_msg.contains(original_content));

        // Make sure the file wasn't changed
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, original_content);
    }

    #[tokio::test]
    async fn test_fs_write_with_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_overwrite.txt");
        let original_content = "Original content";
        let new_content = "New content";

        // First, create the file
        fs::write(&file_path, original_content).await.unwrap();

        // Now attempt to write with overwrite flag
        let fs_write = FSWrite::new(Arc::new(Stub::default()));
        let result = fs_write
            .call(FSWriteInput {
                path: file_path.to_string_lossy().to_string(),
                content: new_content.to_string(),
                overwrite: true,
            })
            .await;

        // Should be successful
        assert!(result.is_ok());
        let success_msg = result.unwrap();

        // Success message should contain expected text
        assert!(success_msg.contains("Successfully wrote"));

        // Verify file was actually overwritten
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, new_content);
    }
}
