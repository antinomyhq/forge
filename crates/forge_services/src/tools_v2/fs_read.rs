use std::sync::Arc;

use forge_app::{Content, FsReadService, ReadOutput};
use forge_domain::ToolDescription;
use forge_tool_macros::ToolDescription;

use crate::utils::assert_absolute_path;
use crate::{FsReadService as _, Infrastructure};

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
#[derive(ToolDescription)]
pub struct ForgeFsRead<F>(Arc<F>);

impl<F: Infrastructure> ForgeFsRead<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> FsReadService for ForgeFsRead<F> {
    async fn read(&self, path: String) -> anyhow::Result<ReadOutput> {
        let path = std::path::Path::new(&path);
        assert_absolute_path(path)?;
        let content = self.0.file_read_service().read_utf8(path).await?;
        Ok(ReadOutput { content: Content::File(content) })
    }
}
