use std::path::Path;
use std::sync::Arc;

use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::utils::assert_absolute_path;
use crate::{FileReadService, Infrastructure};

#[derive(Deserialize, JsonSchema)]
pub struct FSReadInput {
    /// The path of the file to read, always provide absolute paths.
    pub path: String,
}

pub struct FSRead<F: Infrastructure> {
    infra: Arc<F>,
}

impl<F: Infrastructure> FSRead<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

impl<F: Infrastructure> ToolDescription for FSRead<F> {
    fn description(&self) -> String {
        "Request to read the contents of a file at the specified path. Use this when
        you need to examine the contents of an existing file you do not know the
        contents of, for example to analyze code, review text files, or extract
        information from configuration files. Automatically extracts raw text from
        PDF and DOCX files. May not be suitable for other types of binary files, as
        it returns the raw content as a string."
            .to_string()
    }
}

impl<F: Infrastructure> NamedTool for FSRead<F> {
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

        self.infra.file_read_service().read(path).await
    }
}

#[cfg(test)]
mod test {

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::tools::tests::Stub;

    #[tokio::test]
    async fn test_fs_read_success() {
        let test_content = "Hello, World!";
        let fs_read = FSRead::new(Arc::new(Stub::default()));
        let result = fs_read
            .call(FSReadInput { path: "/test/file.txt".to_string() })
            .await
            .unwrap();

        assert_eq!(result, test_content);
    }

    #[tokio::test]
    async fn test_fs_read_nonexistent_file() {
        let fs_read = FSRead::new(Arc::new(Stub::default()));
        let result = fs_read
            .call(FSReadInput { path: "/nonexistent/file.txt".to_string() })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fs_read_empty_file() {
        let fs_read = FSRead::new(Arc::new(Stub::default()));

        let result = fs_read
            .call(FSReadInput { path: "/empty/file.txt".to_string() })
            .await
            .unwrap();

        assert_eq!(result, "");
    }

    #[test]
    fn test_description() {
        assert!(FSRead::new(Arc::new(Stub::default())).description().len() > 100)
    }

    #[tokio::test]
    async fn test_fs_read_relative_path() {
        let fs_read = FSRead::new(Arc::new(Stub::default()));

        let result = fs_read
            .call(FSReadInput { path: "relative/path.txt".to_string() })
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path must be absolute"));
    }
}
