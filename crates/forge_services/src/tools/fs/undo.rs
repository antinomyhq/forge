use std::path::Path;
use std::sync::Arc;

use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::Infrastructure;
use crate::infra::FsSnapshotService;

/// Use this tool to undo the most recent file operation (modify/delete) on a specific file.
/// This tool should be used when:
/// - The user wants to revert a recent file change
/// - A file operation resulted in unintended changes
/// - There's a need to recover from a mistaken file modification or deletion
/// Note: Only the most recent operation on a specific file can be undone.
#[derive(Default, ToolDescription)]
pub struct Undo<F>(Arc<F>);

impl<F> Undo<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

impl<F> NamedTool for Undo<F> {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_fs_undo")
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct UndoInput {
    /// The path of the file to undo the last operation on (absolute path required).
    /// This should be the exact path of the file that was previously modified,
    /// created, or deleted through a Forge file operation.
    pub path: String,
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for Undo<F> {
    type Input = UndoInput;
    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
        let path = Path::new(&input.path);
        self.0.file_snapshot_service().undo_snapshot(path).await?;
        Ok(format!("Successfully undid last operation on path: {}", input.path))
    }
}


#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tempfile::TempDir;
    use crate::tools::registry::tests::Stub;
    use super::*;

    #[tokio::test]
    async fn test_successful_undo() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path().join("success.txt");
        let infra = Arc::new(Stub::default());
        let undo = Undo::new(infra);
        
        // Act
        let result = undo.call(UndoInput {
            path: test_path.to_string_lossy().to_string(),
        }).await;
        
        // Assert
        assert!(result.is_ok(), "Expected successful undo operation");
        assert_eq!(
            result.unwrap(),
            format!("Successfully undid last operation on path: {}", test_path.display()),
            "Unexpected success message"
        );
    }

    #[tokio::test]
    async fn test_tool_name() {
        assert_eq!(
            Undo::<Stub>::tool_name().as_str(),
            "tool_forge_fs_undo",
            "Tool name should match expected value"
        );
    }
}