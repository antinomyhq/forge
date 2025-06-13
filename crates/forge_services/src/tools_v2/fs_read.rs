use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context};
use forge_app::{Content, EnvironmentService, FsReadService, ReadOutput};

use crate::utils::assert_absolute_path;
use crate::{FsReadService as _, Infrastructure};

/// Ensures that the given line range is valid and doesn't exceed the
/// maximum size
///
/// # Arguments
/// * `start_line` - The starting line position
/// * `end_line` - The ending line position
/// * `max_size` - The maximum allowed range size
///
/// # Returns
/// * `Ok(())` if the range is valid and within size limits
/// * `Err(String)` with an error message if the range is invalid or too large
pub fn assert_valid_range(start_line: u64, end_line: u64, max_size: u64) -> anyhow::Result<()> {
    // Check that end_line is not less than start_line
    if end_line < start_line {
        bail!(
            "Invalid range: end line ({end_line}) must not be less than start line ({start_line})"
        )
    }

    // Check that the range size doesn't exceed the maximum
    if end_line.saturating_sub(start_line) > max_size {
        bail!("The requested range exceeds the maximum size of {max_size} lines. Please specify a smaller range.")
    }

    Ok(())
}

/// Reads file contents from the specified absolute path. Ideal for analyzing
/// code, configuration files, documentation, or textual data. Automatically
/// extracts text from PDF and DOCX files, preserving the original formatting.
/// Returns the content as a string. For files larger than 2,000 lines,
/// the tool automatically returns only the first 2,000 lines. You should
/// always rely on this default behavior and avoid specifying custom ranges
/// unless absolutely necessary. If needed, specify a range with the start_line
/// and end_line parameters, ensuring the total range does not exceed 2,000
/// lines. Specifying a range exceeding this limit will result in an error.
/// Binary files are automatically detected and rejected.
pub struct ForgeFsRead<F>(Arc<F>);

impl<F: Infrastructure> ForgeFsRead<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> FsReadService for ForgeFsRead<F> {
    async fn read(
        &self,
        path: String,
        istart_line: Option<u64>,
        iend_line: Option<u64>,
    ) -> anyhow::Result<ReadOutput> {
        let path = Path::new(&path);
        assert_absolute_path(path)?;
        let env = self.0.environment_service().get_environment();

        let start_line = istart_line.unwrap_or(1);
        let end_line = iend_line.unwrap_or(start_line + env.max_read_size);

        // Validate the range size using the module-level assertion function
        assert_valid_range(start_line, end_line, env.max_read_size)?;

        let (content, file_info) = self
            .0
            .file_read_service()
            .range_read_utf8(path, start_line, end_line)
            .await
            .with_context(|| format!("Failed to read file content from {}", path.display()))?;

        Ok(ReadOutput {
            content: Content::File(content),
            start_line: file_info.start_line,
            end_line: file_info.end_line,
            total_lines: file_info.total_lines,
        })
    }
}
