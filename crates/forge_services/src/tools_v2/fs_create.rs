use crate::utils::{assert_absolute_path, format_display_path};
use crate::{FsCreateDirsService, FsMetaService, FsReadService, FsWriteService, Infrastructure};
use anyhow::Context;
use bytes::Bytes;
use console::strip_ansi_codes;
use forge_app::{EnvironmentService, FsCreateOutput, FsCreateService};
use forge_display::DiffFormat;
use forge_domain::ToolDescription;
use forge_tool_macros::ToolDescription;
use std::path::Path;
use std::sync::Arc;

/// Use it to create a new file at a specified path with the provided content.
/// Always provide absolute paths for file locations. The tool
/// automatically handles the creation of any missing intermediary directories
/// in the specified path.
/// IMPORTANT: DO NOT attempt to use this tool to move or rename files, use the
/// shell tool instead.
#[derive(ToolDescription)]
pub struct ForgeFsCreate<F>(Arc<F>);

impl<F: Infrastructure> ForgeFsCreate<F> {
    pub fn new(infra: Arc<F>) -> Self {
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
}

#[async_trait::async_trait]
impl<F: Infrastructure> FsCreateService for ForgeFsCreate<F> {
    async fn create(
        &self,
        path: String,
        content: String,
        overwrite: bool,
    ) -> anyhow::Result<FsCreateOutput> {
        let path = Path::new(&path);
        assert_absolute_path(path)?;
        // Validate file content if it's a supported language file
        let syntax_warning = super::syn::validate(&path, &content);
        if let Some(parent) = Path::new(&path).parent() {
            self.0
                .create_dirs_service()
                .create_dirs(parent)
                .await
                .with_context(|| format!("Failed to create directories: {}", path.display()))?;
        }
        // Check if the file exists
        let file_exists = self.0.file_meta_service().is_file(path).await?;

        // If file exists and overwrite flag is not set, return an error with the
        // existing content
        if file_exists && !overwrite {
            let existing_content = self.0.file_read_service().read_utf8(path).await?;
            return Err(anyhow::anyhow!(
                "File already exists at {}. If you need to overwrite it, set overwrite to true.\n\nExisting content:\n{}",
                path.display(),
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

        let chars = content.len();

        // Write file only after validation passes and directories are created
        self.0
            .file_write_service()
            .write(path, Bytes::from(content))
            .await?;

        let new_content = self.0.file_read_service().read_utf8(path).await?;
        let diff = DiffFormat::format(&old_content, &new_content);
        let formatted_path = self.format_display_path(path)?;

        Ok(FsCreateOutput {
            path: path.display().to_string(),
            exists: file_exists,
            chars,
            warning: syntax_warning.map(|v| v.to_string()),
            diff: strip_ansi_codes(&diff).to_string(),
            formatted_path,
        })
    }
}
