use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_app::EnvironmentService;
use forge_display::TitleFormat;
use forge_domain::{
    ExecutableTool, FSFileInfoInput, NamedTool, ToolCallContext, ToolDescription, ToolName,
    ToolOutput,
};
use forge_tool_macros::ToolDescription;

use crate::utils::{assert_absolute_path, format_display_path};
use crate::Infrastructure;

/// Request to retrieve detailed metadata about a file or directory at the
/// specified path. Returns comprehensive information including size, creation
/// time, last modified time, permissions, and type. Path must be absolute. Use
/// this when you need to understand file characteristics without reading the
/// actual content.
#[derive(ToolDescription)]
pub struct FSFileInfo<F> {
    infra: Arc<F>,
}

impl<F: Infrastructure> FSFileInfo<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

impl<F> NamedTool for FSFileInfo<F> {
    fn tool_name() -> ToolName {
        ToolName::new("forge_tool_fs_info")
    }
}

impl<F: Infrastructure> FSFileInfo<F> {
    /// Formats a path for display, converting absolute paths to relative when
    /// possible
    ///
    /// If the path starts with the current working directory, returns a
    /// relative path. Otherwise, returns the original absolute path.
    fn format_display_path(&self, path: &Path) -> anyhow::Result<String> {
        // Get the current working directory
        let env = self.infra.environment_service().get_environment();
        let cwd = env.cwd.as_path();

        // Use the shared utility function
        format_display_path(path, cwd)
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for FSFileInfo<F> {
    type Input = FSFileInfoInput;

    async fn call(
        &self,
        context: &mut ToolCallContext,
        input: Self::Input,
    ) -> anyhow::Result<ToolOutput> {
        let path = Path::new(&input.path);
        assert_absolute_path(path)?;

        let meta = tokio::fs::metadata(&input.path)
            .await
            .with_context(|| format!("Failed to get metadata for '{}'", input.path))?;

        context
            .send_text(TitleFormat::debug("Info").title(self.format_display_path(path)?))
            .await?;
        Ok(ToolOutput::text(format!("{meta:?}")))
    }
}

#[cfg(test)]
mod test {
    use tokio::fs;

    use super::*;
    use crate::utils::{TempDir, ToolContentExtension};

    #[tokio::test]
    async fn test_fs_file_info_on_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content").await.unwrap();

        // Create and use the stub infrastructure
        let stub = Arc::new(crate::tools::registry::tests::Stub::default());
        let fs_info = FSFileInfo::new(stub);
        let result = fs_info
            .call(
                &mut ToolCallContext::default(),
                FSFileInfoInput {
                    path: file_path.to_string_lossy().to_string(),
                    explanation: None,
                },
            )
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

        // Create and use the stub infrastructure
        let stub = Arc::new(crate::tools::registry::tests::Stub::default());
        let fs_info = FSFileInfo::new(stub);
        let result = fs_info
            .call(
                &mut ToolCallContext::default(),
                FSFileInfoInput {
                    path: dir_path.to_string_lossy().to_string(),
                    explanation: None,
                },
            )
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

        // Create and use the stub infrastructure
        let stub = Arc::new(crate::tools::registry::tests::Stub::default());
        let fs_info = FSFileInfo::new(stub);
        let result = fs_info
            .call(
                &mut ToolCallContext::default(),
                FSFileInfoInput {
                    path: nonexistent_path.to_string_lossy().to_string(),
                    explanation: None,
                },
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fs_file_info_relative_path() {
        // Create and use the stub infrastructure
        let stub = Arc::new(crate::tools::registry::tests::Stub::default());
        let fs_info = FSFileInfo::new(stub);
        let result = fs_info
            .call(
                &mut ToolCallContext::default(),
                FSFileInfoInput { path: "relative/path.txt".to_string(), explanation: None },
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path must be absolute"));
    }
}
