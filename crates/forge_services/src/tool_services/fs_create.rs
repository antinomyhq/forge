use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use forge_app::{FsCreateOutput, FsCreateService};

use crate::utils::assert_absolute_path;
use crate::{FileDirectoryInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra, tool_services};

/// Use it to create a new file at a specified path with the provided content.
/// Always provide absolute paths for file locations. The tool
/// automatically handles the creation of any missing intermediary directories
/// in the specified path.
/// IMPORTANT: DO NOT attempt to use this tool to move or rename files, use the
/// shell tool instead.
pub struct ForgeFsCreate<F>(Arc<F>);

impl<F> ForgeFsCreate<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

#[async_trait::async_trait]
impl<F: FileDirectoryInfra + FileInfoInfra + FileReaderInfra + FileWriterInfra + Send + Sync>
    FsCreateService for ForgeFsCreate<F>
{
    async fn create(
        &self,
        path: String,
        content: String,
        overwrite: bool,
        capture_snapshot: bool,
    ) -> anyhow::Result<FsCreateOutput> {
        let path = Path::new(&path);
        assert_absolute_path(path)?;
        // Validate file content if it's a supported language file
        let syntax_warning = tool_services::syn::validate(path, &content);
        if let Some(parent) = Path::new(&path).parent() {
            self.0
                .create_dirs(parent)
                .await
                .with_context(|| format!("Failed to create directories: {}", path.display()))?;
        }
        // Check if the file exists
        let file_exists = self.0.is_file(path).await?;

        // If file exists and overwrite flag is not set, create a temporary file
        // and return an error with helpful next steps
        if file_exists && !overwrite {
            return Ok(FsCreateOutput::AttemptToEditWithoutOverwrite);
        }

        // record the file content before they're modified
        let old_content = if file_exists && overwrite {
            Some(self.0.read_utf8(path).await?)
        } else {
            None
        };

        // Write file only after validation passes and directories are created
        self.0
            .write(path, Bytes::from(content), capture_snapshot)
            .await?;

        Ok(FsCreateOutput::Success {
            path: path.display().to_string(),
            before: old_content,
            warning: syntax_warning.map(|v| v.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use bytes::Bytes;
    use forge_app::FsCreateService;
    use pretty_assertions::assert_eq;

    use super::ForgeFsCreate;

    // Mock infrastructure for testing
    #[derive(Default)]
    struct MockInfra {
        files: std::collections::HashMap<String, String>,
        file_exists: bool,
    }

    impl MockInfra {
        fn with_existing_file(path: &str, content: &str) -> Self {
            let mut files = std::collections::HashMap::new();
            files.insert(path.to_string(), content.to_string());
            Self { files, file_exists: true }
        }
    }

    #[async_trait::async_trait]
    impl crate::FileDirectoryInfra for MockInfra {
        async fn create_dirs(&self, _path: &Path) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl crate::FileInfoInfra for MockInfra {
        async fn is_file(&self, _path: &Path) -> anyhow::Result<bool> {
            Ok(self.file_exists)
        }

        async fn exists(&self, _path: &Path) -> anyhow::Result<bool> {
            Ok(self.file_exists)
        }

        async fn file_size(&self, _path: &Path) -> anyhow::Result<u64> {
            Ok(0)
        }
    }

    #[async_trait::async_trait]
    impl crate::FileReaderInfra for MockInfra {
        async fn read(&self, _path: &Path) -> anyhow::Result<Vec<u8>> {
            Ok(vec![])
        }

        async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
            self.files
                .get(&path.display().to_string())
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("File not found"))
        }

        async fn range_read_utf8(
            &self,
            _path: &Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, forge_fs::FileInfo)> {
            Ok((String::new(), forge_fs::FileInfo::new(1, 1, 1)))
        }
    }

    #[async_trait::async_trait]
    impl crate::FileWriterInfra for MockInfra {
        async fn write(
            &self,
            _path: &Path,
            _content: Bytes,
            _capture_snapshot: bool,
        ) -> anyhow::Result<()> {
            // Just verify the path for testing
            Ok(())
        }

        async fn write_temp(
            &self,
            _prefix: &str,
            _ext: &str,
            _content: &str,
        ) -> anyhow::Result<PathBuf> {
            Ok(PathBuf::from("/tmp/test"))
        }
    }

    #[tokio::test]
    async fn test_create_temp_file_when_file_exists_and_overwrite_false() {
        let fixture = ForgeFsCreate::new(Arc::new(MockInfra::with_existing_file(
            "/home/user/test.rs",
            "existing content",
        )));

        let actual = fixture
            .create(
                "/home/user/test.rs".to_string(),
                "new content".to_string(),
                false,
                false,
            )
            .await
            .unwrap();

        let expected = forge_app::FsCreateOutput::AttemptToEditWithoutOverwrite;

        match (actual, expected) {
            (
                forge_app::FsCreateOutput::AttemptToEditWithoutOverwrite,
                forge_app::FsCreateOutput::AttemptToEditWithoutOverwrite,
            ) => (),
            _ => panic!("Expected Failure variant"),
        }
    }

    #[tokio::test]
    async fn test_create_new_file_when_file_does_not_exist() {
        let fixture = ForgeFsCreate::new(Arc::new(MockInfra::default()));

        let actual = fixture
            .create(
                "/home/user/new_file.rs".to_string(),
                "new content".to_string(),
                false,
                false,
            )
            .await
            .unwrap();

        let expected = forge_app::FsCreateOutput::Success {
            path: "/home/user/new_file.rs".to_string(),
            before: None,
            warning: Some("Syntax error found in file with extension rs. Hint: Please retry in raw mode without HTML-encoding angle brackets.".to_string()),
        };

        match (actual, expected) {
            (
                forge_app::FsCreateOutput::Success {
                    path: actual_path,
                    before: actual_before,
                    warning: actual_warning,
                },
                forge_app::FsCreateOutput::Success {
                    path: expected_path,
                    before: expected_before,
                    warning: expected_warning,
                },
            ) => {
                assert_eq!(actual_path, expected_path);
                assert_eq!(actual_before, expected_before);
                assert_eq!(actual_warning, expected_warning);
            }
            _ => panic!("Expected Success variant"),
        }
    }

    #[tokio::test]
    async fn test_overwrite_existing_file_when_overwrite_true() {
        let fixture = ForgeFsCreate::new(Arc::new(MockInfra::with_existing_file(
            "/home/user/test.rs",
            "old content",
        )));

        let actual = fixture
            .create(
                "/home/user/test.rs".to_string(),
                "new content".to_string(),
                true,
                false,
            )
            .await
            .unwrap();

        let expected = forge_app::FsCreateOutput::Success {
            path: "/home/user/test.rs".to_string(),
            before: Some("old content".to_string()),
            warning: Some("Syntax error found in file with extension rs. Hint: Please retry in raw mode without HTML-encoding angle brackets.".to_string()),
        };

        match (actual, expected) {
            (
                forge_app::FsCreateOutput::Success {
                    path: actual_path,
                    before: actual_before,
                    warning: actual_warning,
                },
                forge_app::FsCreateOutput::Success {
                    path: expected_path,
                    before: expected_before,
                    warning: expected_warning,
                },
            ) => {
                assert_eq!(actual_path, expected_path);
                assert_eq!(actual_before, expected_before);
                assert_eq!(actual_warning, expected_warning);
            }
            _ => panic!("Expected Success variant"),
        }
    }
}
