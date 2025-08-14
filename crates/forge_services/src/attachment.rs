use std::sync::Arc;

use forge_app::AttachmentService;
use forge_app::domain::{Attachment, AttachmentContent, FileTag, Image};

use crate::range::resolve_range;
use crate::{EnvironmentInfra, FileReaderInfra};

#[derive(Clone)]
pub struct ForgeChatRequest<F> {
    infra: Arc<F>,
}

impl<F: FileReaderInfra + EnvironmentInfra> ForgeChatRequest<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    async fn prepare_attachments(&self, paths: Vec<FileTag>) -> anyhow::Result<Vec<Attachment>> {
        futures::future::join_all(paths.into_iter().map(|v| self.populate_attachments(v)))
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()
    }

    async fn populate_attachments(&self, tag: FileTag) -> anyhow::Result<Attachment> {
        let mut path = tag.as_ref().to_path_buf();
        let extension = path.extension().map(|v| v.to_string_lossy().to_string());

        if !path.is_absolute() {
            path = self.infra.get_environment().cwd.join(path);
        }

        // Determine file type (text or image with format)
        let mime_type = extension.and_then(|ext| match ext.as_str() {
            "jpeg" | "jpg" => Some("image/jpeg".to_string()),
            "png" => Some("image/png".to_string()),
            "webp" => Some("image/webp".to_string()),
            _ => None,
        });

        //NOTE: Apply the same slicing as file reads for text content
        let content = match mime_type {
            Some(mime_type) => {
                AttachmentContent::Image(Image::new_bytes(self.infra.read(&path).await?, mime_type))
            }
            None => {
                let env = self.infra.get_environment();

                let start = tag.loc.as_ref().and_then(|loc| loc.start);
                let end = tag.loc.as_ref().and_then(|loc| loc.end);
                let (start_line, end_line) = resolve_range(start, end, env.max_read_size);

                let (file_content, file_info) = self
                    .infra
                    .range_read_utf8(&path, start_line, end_line)
                    .await?;

                AttachmentContent::FileContent {
                    content: file_content,
                    start_line: file_info.start_line,
                    end_line: file_info.end_line,
                    total_lines: file_info.total_lines,
                }
            }
        };

        Ok(Attachment { content, path: path.to_string_lossy().to_string() })
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + EnvironmentInfra> AttachmentService for ForgeChatRequest<F> {
    async fn attachments(&self, url: &str) -> anyhow::Result<Vec<Attachment>> {
        self.prepare_attachments(Attachment::parse_all(url)).await
    }
}

#[cfg(test)]
pub mod tests {
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use base64::Engine;
    use bytes::Bytes;
    use forge_app::AttachmentService;
    use forge_app::domain::{
        AttachmentContent, CommandOutput, Environment, ToolDefinition, ToolName, ToolOutput,
    };
    use forge_snaps::Snapshot;
    use serde_json::Value;
    use url::Url;

    use crate::attachment::ForgeChatRequest;
    use crate::{
        CommandInfra, EnvironmentInfra, FileDirectoryInfra, FileInfoInfra, FileReaderInfra,
        FileRemoverInfra, FileWriterInfra, McpClientInfra, McpServerInfra, SnapshotInfra,
        UserInfra,
    };

    #[derive(Debug)]
    pub struct MockEnvironmentInfra {}

    #[async_trait::async_trait]
    impl EnvironmentInfra for MockEnvironmentInfra {
        fn get_environment(&self) -> Environment {
            let max_bytes: f64 = 250.0 * 1024.0; // 250 KB
            Environment {
                os: "test".to_string(),
                pid: 12345,
                cwd: PathBuf::from("/test"),
                home: Some(PathBuf::from("/home/test")),
                shell: "bash".to_string(),
                base_path: PathBuf::from("/base"),
                retry_config: Default::default(),
                max_search_lines: 25,
                max_search_result_bytes: max_bytes.ceil() as usize, // 0.25 MB
                fetch_truncation_limit: 0,
                stdout_max_prefix_length: 0,
                stdout_max_suffix_length: 0,
                stdout_max_line_length: 2000,
                max_read_size: 2000,
                http: Default::default(),
                max_file_size: 10_000_000,
                forge_api_url: Url::parse("http://forgecode.dev/api").unwrap(),
            }
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }
    }

    impl MockFileService {
        pub fn new() -> Self {
            let mut files = HashMap::new();
            // Add some mock files
            files.insert(
                PathBuf::from("/test/file1.txt"),
                "This is a text file content".to_string(),
            );
            files.insert(
                PathBuf::from("/test/image.png"),
                "mock-binary-content".to_string(),
            );
            files.insert(
                PathBuf::from("/test/image with spaces.jpg"),
                "mock-jpeg-content".to_string(),
            );

            let binary_exts = [
                "exe", "dll", "so", "dylib", "bin", "obj", "o", "class", "pyc", "jar", "war",
                "ear", "zip", "tar", "gz", "rar", "7z", "iso", "img", "pdf", "doc", "docx", "xls",
                "xlsx", "ppt", "pptx", "bmp", "ico", "mp3", "mp4", "avi", "mov", "sqlite", "db",
                "bin",
            ];
            let binary_exts = binary_exts.into_iter().map(|s| s.to_string()).collect();

            Self {
                files: Mutex::new(
                    files
                        .into_iter()
                        .map(|(a, b)| (a, Bytes::from(b)))
                        .collect::<Vec<_>>(),
                ),
                binary_exts,
            }
        }

        pub fn add_file(&self, path: PathBuf, content: String) {
            let mut files = self.files.lock().unwrap();
            files.push((path, Bytes::from_owner(content)));
        }
    }

    #[async_trait::async_trait]
    impl FileReaderInfra for MockFileService {
        async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
            let files = self.files.lock().unwrap();
            match files.iter().find(|v| v.0 == path) {
                Some((_, content)) => {
                    let bytes = content.clone();
                    String::from_utf8(bytes.to_vec())
                        .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in file: {:?}: {}", path, e))
                }
                None => Err(anyhow::anyhow!("File not found: {:?}", path)),
            }
        }

        async fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
            let files = self.files.lock().unwrap();
            match files.iter().find(|v| v.0 == path) {
                Some((_, content)) => Ok(content.to_vec()),
                None => Err(anyhow::anyhow!("File not found: {:?}", path)),
            }
        }

        async fn range_read_utf8(
            &self,
            path: &Path,
            start_line: u64,
            end_line: u64,
        ) -> anyhow::Result<(String, forge_fs::FileInfo)> {
            // Read the full content first
            let full_content = self.read_utf8(path).await?;
            let all_lines: Vec<&str> = full_content.lines().collect();

            // Apply range filtering based on parameters
            let start_idx = start_line.saturating_sub(1) as usize;
            let end_idx = if end_line > 0 {
                std::cmp::min(end_line as usize, all_lines.len())
            } else {
                all_lines.len()
            };

            let filtered_lines = if start_idx < all_lines.len() {
                &all_lines[start_idx..end_idx]
            } else {
                &[]
            };

            let filtered_content = filtered_lines.join("\n");
            let actual_start = if filtered_lines.is_empty() {
                0
            } else {
                start_line
            };
            let actual_end = if filtered_lines.is_empty() {
                0
            } else {
                start_idx as u64 + filtered_lines.len() as u64
            };

            Ok((
                filtered_content,
                forge_fs::FileInfo::new(actual_start, actual_end, all_lines.len() as u64),
            ))
        }
    }

    #[derive(Debug)]
    pub struct MockFileService {
        files: Mutex<Vec<(PathBuf, Bytes)>>,
        binary_exts: HashSet<String>,
    }

    #[async_trait::async_trait]
    impl FileRemoverInfra for MockFileService {
        async fn remove(&self, path: &Path) -> anyhow::Result<()> {
            if !self.exists(path).await? {
                return Err(anyhow::anyhow!("File not found: {:?}", path));
            }
            self.files.lock().unwrap().retain(|(p, _)| p != path);
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl FileDirectoryInfra for MockFileService {
        async fn create_dirs(&self, path: &Path) -> anyhow::Result<()> {
            self.files
                .lock()
                .unwrap()
                .push((path.to_path_buf(), Bytes::new()));
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl FileWriterInfra for MockFileService {
        async fn write(
            &self,
            path: &Path,
            contents: Bytes,
            _capture_snapshot: bool,
        ) -> anyhow::Result<()> {
            let index = self.files.lock().unwrap().iter().position(|v| v.0 == path);
            if let Some(index) = index {
                self.files.lock().unwrap().remove(index);
            }
            self.files
                .lock()
                .unwrap()
                .push((path.to_path_buf(), contents));
            Ok(())
        }

        async fn write_temp(&self, _: &str, _: &str, content: &str) -> anyhow::Result<PathBuf> {
            let temp_dir = crate::utils::TempDir::new().unwrap();
            let path = temp_dir.path();

            self.write(&path, content.to_string().into(), false).await?;

            Ok(path)
        }
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    pub struct MockSnapService;

    #[async_trait::async_trait]
    impl SnapshotInfra for MockSnapService {
        async fn create_snapshot(&self, _: &Path) -> anyhow::Result<Snapshot> {
            unimplemented!()
        }

        async fn undo_snapshot(&self, _: &Path) -> anyhow::Result<()> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FileInfoInfra for MockFileService {
        async fn is_file(&self, path: &Path) -> anyhow::Result<bool> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .filter(|v| v.0.extension().is_some())
                .any(|(p, _)| p == path))
        }

        async fn is_binary(&self, _path: &Path) -> anyhow::Result<bool> {
            let ext = _path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase());
            Ok(ext.map(|e| self.binary_exts.contains(&e)).unwrap_or(false))
        }

        async fn exists(&self, path: &Path) -> anyhow::Result<bool> {
            Ok(self.files.lock().unwrap().iter().any(|(p, _)| p == path))
        }

        async fn file_size(&self, path: &Path) -> anyhow::Result<u64> {
            let files = self.files.lock().unwrap();
            if let Some((_, content)) = files.iter().find(|(p, _)| p == path) {
                Ok(content.len() as u64)
            } else {
                Err(anyhow::anyhow!("File not found: {}", path.display()))
            }
        }
    }

    #[async_trait::async_trait]
    impl McpClientInfra for () {
        async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
            Ok(vec![])
        }

        async fn call(&self, _: &ToolName, _: Value) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::default())
        }
    }

    #[async_trait::async_trait]
    impl McpServerInfra for () {
        type Client = ();

        async fn connect(
            &self,
            _: forge_app::domain::McpServerConfig,
        ) -> anyhow::Result<Self::Client> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl CommandInfra for () {
        async fn execute_command(
            &self,
            command: String,
            working_dir: PathBuf,
        ) -> anyhow::Result<CommandOutput> {
            // For test purposes, we'll create outputs that match what the shell tests
            // expect Check for common command patterns
            if command == "echo 'Hello, World!'" {
                // When the test_shell_echo looks for this specific command
                // It's expecting to see "Mock command executed successfully"
                return Ok(CommandOutput {
                    stdout: "Mock command executed successfully\n".to_string(),
                    stderr: "".to_string(),
                    command,
                    exit_code: Some(0),
                });
            } else if command.contains("echo") {
                if command.contains(">") && command.contains(">&2") {
                    // Commands with both stdout and stderr
                    let stdout = if command.contains("to stdout") {
                        "to stdout\n"
                    } else {
                        "stdout output\n"
                    };
                    let stderr = if command.contains("to stderr") {
                        "to stderr\n"
                    } else {
                        "stderr output\n"
                    };
                    return Ok(CommandOutput {
                        stdout: stdout.to_string(),
                        stderr: stderr.to_string(),
                        command,
                        exit_code: Some(0),
                    });
                } else if command.contains(">&2") {
                    // Command with only stderr
                    let content = command.split("echo").nth(1).unwrap_or("").trim();
                    let content = content.trim_matches(|c| c == '\'' || c == '"');
                    return Ok(CommandOutput {
                        stdout: "".to_string(),
                        stderr: format!("{content}\n"),
                        command,
                        exit_code: Some(0),
                    });
                } else {
                    // Standard echo command
                    let content = if command == "echo ''" {
                        "\n".to_string()
                    } else if command.contains("&&") {
                        // Multiple commands
                        "first\nsecond\n".to_string()
                    } else if command.contains("$PATH") {
                        // PATH command returns a mock path
                        "/usr/bin:/bin:/usr/sbin:/sbin\n".to_string()
                    } else {
                        let parts: Vec<&str> = command.split("echo").collect();
                        if parts.len() > 1 {
                            let content = parts[1].trim();
                            // Remove quotes if present
                            let content = content.trim_matches(|c| c == '\'' || c == '"');
                            format!("{content}\n")
                        } else {
                            "Hello, World!\n".to_string()
                        }
                    };

                    return Ok(CommandOutput {
                        stdout: content,
                        stderr: "".to_string(),
                        command,
                        exit_code: Some(0),
                    });
                }
            } else if command == "pwd" || command == "cd" {
                // Return working directory for pwd/cd commands
                return Ok(CommandOutput {
                    stdout: format!("{working_dir}\n", working_dir = working_dir.display()),
                    stderr: "".to_string(),
                    command,
                    exit_code: Some(0),
                });
            } else if command == "true" {
                // true command returns success with no output
                return Ok(CommandOutput {
                    stdout: "".to_string(),
                    stderr: "".to_string(),
                    command,
                    exit_code: Some(0),
                });
            } else if command.starts_with("/bin/ls") || command.contains("whoami") {
                // Full path commands
                return Ok(CommandOutput {
                    stdout: "user\n".to_string(),
                    stderr: "".to_string(),
                    command,
                    exit_code: Some(0),
                });
            } else if command == "non_existent_command" {
                // Command not found
                return Ok(CommandOutput {
                    stdout: "".to_string(),
                    stderr: "command not found: non_existent_command\n".to_string(),
                    command,
                    exit_code: Some(-1),
                });
            }

            // Default response for other commands
            Ok(CommandOutput {
                stdout: "Mock command executed successfully\n".to_string(),
                stderr: "".to_string(),
                command,
                exit_code: Some(0),
            })
        }

        async fn execute_command_raw(
            &self,
            _: &str,
            _: PathBuf,
        ) -> anyhow::Result<std::process::ExitStatus> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl UserInfra for () {
        /// Prompts the user with question
        async fn prompt_question(&self, question: &str) -> anyhow::Result<Option<String>> {
            // For testing, we can just return the question as the answer
            Ok(Some(question.to_string()))
        }

        /// Prompts the user to select a single option from a list
        async fn select_one<T: std::fmt::Display + Send + 'static>(
            &self,
            _: &str,
            options: Vec<T>,
        ) -> anyhow::Result<Option<T>> {
            // For testing, we can just return the first option
            if options.is_empty() {
                return Err(anyhow::anyhow!("No options provided"));
            }
            Ok(Some(options.into_iter().next().unwrap()))
        }

        /// Prompts the user to select multiple options from a list
        async fn select_many<T: std::fmt::Display + Clone + Send + 'static>(
            &self,
            _: &str,
            options: Vec<T>,
        ) -> anyhow::Result<Option<Vec<T>>> {
            // For testing, we can just return all options
            if options.is_empty() {
                return Err(anyhow::anyhow!("No options provided"));
            }
            Ok(Some(options))
        }
    }

    // Create a composite mock service that implements the required traits
    #[derive(Debug, Clone)]
    pub struct MockCompositeService {
        file_service: Arc<MockFileService>,
        env_service: Arc<MockEnvironmentInfra>,
    }

    impl MockCompositeService {
        pub fn new() -> Self {
            Self {
                file_service: Arc::new(MockFileService::new()),
                env_service: Arc::new(MockEnvironmentInfra {}),
            }
        }

        pub fn add_file(&self, path: PathBuf, content: String) {
            self.file_service.add_file(path, content);
        }
    }

    #[async_trait::async_trait]
    impl FileReaderInfra for MockCompositeService {
        async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
            self.file_service.read_utf8(path).await
        }

        async fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
            self.file_service.read(path).await
        }

        async fn range_read_utf8(
            &self,
            path: &Path,
            start_line: u64,
            end_line: u64,
        ) -> anyhow::Result<(String, forge_fs::FileInfo)> {
            self.file_service
                .range_read_utf8(path, start_line, end_line)
                .await
        }
    }

    #[async_trait::async_trait]
    impl EnvironmentInfra for MockCompositeService {
        fn get_environment(&self) -> Environment {
            self.env_service.get_environment()
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }
    }

    #[tokio::test]
    async fn test_add_url_with_text_file() {
        // Setup
        let infra = Arc::new(MockCompositeService::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with a text file path in chat message
        let url = "@[/test/file1.txt]".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert
        // Text files should be included in the attachments
        assert_eq!(attachments.len(), 1);
        let attachment = attachments.first().unwrap();
        assert_eq!(attachment.path, "/test/file1.txt");

        // Check that the content contains our original text and has range information
        assert!(attachment.content.contains("This is a text file content"));
    }

    #[tokio::test]
    async fn test_add_url_with_image() {
        // Setup
        let infra = Arc::new(MockCompositeService::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with an image file
        let url = "@[/test/image.png]".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert
        assert_eq!(attachments.len(), 1);
        let attachment = attachments.first().unwrap();
        assert_eq!(attachment.path, "/test/image.png");

        // Base64 content should be the encoded mock binary content with proper data URI
        // format
        let expected_base64 =
            base64::engine::general_purpose::STANDARD.encode("mock-binary-content");
        assert_eq!(
            attachment.content.as_image().unwrap().url().as_str(),
            format!("data:image/png;base64,{expected_base64}")
        );
    }

    #[tokio::test]
    async fn test_add_url_with_jpg_image_with_spaces() {
        // Setup
        let infra = Arc::new(MockCompositeService::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with an image file that has spaces in the path
        let url = "@[/test/image with spaces.jpg]".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert
        assert_eq!(attachments.len(), 1);
        let attachment = attachments.first().unwrap();
        assert_eq!(attachment.path, "/test/image with spaces.jpg");

        // Base64 content should be the encoded mock jpeg content with proper data URI
        // format
        let expected_base64 = base64::engine::general_purpose::STANDARD.encode("mock-jpeg-content");
        assert_eq!(
            attachment.content.as_image().unwrap().url().as_str(),
            format!("data:image/jpeg;base64,{expected_base64}")
        );
    }

    #[tokio::test]
    async fn test_add_url_with_multiple_files() {
        // Setup
        let infra = Arc::new(MockCompositeService::new());

        // Add an extra file to our mock service
        infra.add_file(
            PathBuf::from("/test/file2.txt"),
            "This is another text file".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with multiple files mentioned
        let url = "@[/test/file1.txt] @[/test/file2.txt] @[/test/image.png]".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert
        // All files should be included in the attachments
        assert_eq!(attachments.len(), 3);

        // Verify that each expected file is in the attachments
        let has_file1 = attachments.iter().any(|a| {
            a.path == "/test/file1.txt"
                && matches!(a.content, AttachmentContent::FileContent { .. })
        });
        let has_file2 = attachments.iter().any(|a| {
            a.path == "/test/file2.txt"
                && matches!(a.content, AttachmentContent::FileContent { .. })
        });
        let has_image = attachments.iter().any(|a| {
            a.path == "/test/image.png" && matches!(a.content, AttachmentContent::Image(_))
        });

        assert!(has_file1, "Missing file1.txt in attachments");
        assert!(has_file2, "Missing file2.txt in attachments");
        assert!(has_image, "Missing image.png in attachments");
    }

    #[tokio::test]
    async fn test_add_url_with_nonexistent_file() {
        // Setup
        let infra = Arc::new(MockCompositeService::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with a file that doesn't exist
        let url = "@[/test/nonexistent.txt]".to_string();

        // Execute - Let's handle the error properly
        let result = chat_request.attachments(&url).await;

        // Assert - we expect an error for nonexistent files
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[tokio::test]
    async fn test_add_url_empty() {
        // Setup
        let infra = Arc::new(MockCompositeService::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with an empty message
        let url = "".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert - no attachments
        assert_eq!(attachments.len(), 0);
    }

    #[tokio::test]
    async fn test_add_url_with_unsupported_extension() {
        // Setup
        let infra = Arc::new(MockCompositeService::new());

        // Add a file with unsupported extension
        infra.add_file(
            PathBuf::from("/test/unknown.xyz"),
            "Some content".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with the file
        let url = "@[/test/unknown.xyz]".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert - should be treated as text
        assert_eq!(attachments.len(), 1);
        let attachment = attachments.first().unwrap();
        assert_eq!(attachment.path, "/test/unknown.xyz");

        // Check that the content contains our original text and has range information
        assert!(attachment.content.contains("Some content"));
    }

    #[tokio::test]
    async fn test_attachment_range_information() {
        // Setup
        let infra = Arc::new(MockCompositeService::new());

        // Add a multi-line file to test range information
        infra.add_file(
            PathBuf::from("/test/multiline.txt"),
            "Line 1\nLine 2\nLine 3\nLine 4\nLine 5".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());
        let url = "@[/test/multiline.txt]".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert
        assert_eq!(attachments.len(), 1);
        let attachment = attachments.first().unwrap();

        // Verify range information is populated
        let range_info = attachment.content.range_info();
        assert!(
            range_info.is_some(),
            "Range information should be present for file content"
        );

        let (start_line, end_line, total_lines) = range_info.unwrap();
        assert_eq!(start_line, 1, "Start line should be 1");
        assert!(end_line >= start_line, "End line should be >= start line");
        assert!(total_lines >= end_line, "Total lines should be >= end line");

        // Verify content is accessible through helper method
        let file_content = attachment.content.file_content();
        assert!(file_content.is_some(), "File content should be accessible");
        assert!(
            file_content.unwrap().contains("Line 1"),
            "Should contain file content"
        );
    }

    // Range functionality tests
    #[tokio::test]
    async fn test_range_functionality_single_line() {
        let infra = Arc::new(MockCompositeService::new());

        // Add a multi-line test file
        infra.add_file(
            PathBuf::from("/test/multiline.txt"),
            "Line 1\nLine 2\nLine 3\nLine 4\nLine 5".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test reading line 2 only
        let url = "@[/test/multiline.txt:2:2]";
        let attachments = chat_request.attachments(&url).await.unwrap();

        assert_eq!(attachments.len(), 1);
        assert_eq!(
            attachments[0].content,
            AttachmentContent::FileContent {
                content: "Line 2".to_string(),
                start_line: 2,
                end_line: 2,
                total_lines: 5,
            }
        );
    }

    #[tokio::test]
    async fn test_range_functionality_multiple_lines() {
        let infra = Arc::new(MockCompositeService::new());

        // Add a multi-line test file
        infra.add_file(
            PathBuf::from("/test/range_test.txt"),
            "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test reading lines 2-4
        let url = "@[/test/range_test.txt:2:4]";
        let attachments = chat_request.attachments(&url).await.unwrap();

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments.len(), 1);
        assert_eq!(
            attachments[0].content,
            AttachmentContent::FileContent {
                content: "Line 2\nLine 3\nLine 4".to_string(),
                start_line: 2,
                end_line: 4,
                total_lines: 6,
            }
        );
    }

    #[tokio::test]
    async fn test_range_functionality_from_start() {
        let infra = Arc::new(MockCompositeService::new());

        infra.add_file(
            PathBuf::from("/test/start_range.txt"),
            "First\nSecond\nThird\nFourth".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test reading from start to line 2
        let url = "@[/test/start_range.txt:1:2]";
        let attachments = chat_request.attachments(&url).await.unwrap();
        assert_eq!(
            attachments[0].content,
            AttachmentContent::FileContent {
                content: "First\nSecond".to_string(),
                start_line: 1,
                end_line: 2,
                total_lines: 4,
            }
        );
    }

    #[tokio::test]
    async fn test_range_functionality_to_end() {
        let infra = Arc::new(MockCompositeService::new());

        infra.add_file(
            PathBuf::from("/test/end_range.txt"),
            "Alpha\nBeta\nGamma\nDelta\nEpsilon".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test reading from line 3 to end
        let url = "@[/test/end_range.txt:3:5]";
        let attachments = chat_request.attachments(&url).await.unwrap();
        assert_eq!(
            attachments[0].content,
            AttachmentContent::FileContent {
                content: "Gamma\nDelta\nEpsilon".to_string(),
                start_line: 3,
                end_line: 5,
                total_lines: 5,
            }
        );
    }

    #[tokio::test]
    async fn test_range_functionality_edge_cases() {
        let infra = Arc::new(MockCompositeService::new());

        infra.add_file(
            PathBuf::from("/test/edge_case.txt"),
            "Only line".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test reading beyond file length
        let url = "@[/test/edge_case.txt:1:10]";
        let attachments = chat_request.attachments(&url).await.unwrap();
        assert_eq!(
            attachments[0].content,
            AttachmentContent::FileContent {
                content: "Only line".to_string(),
                start_line: 1,
                end_line: 1,
                total_lines: 1,
            }
        );
    }

    #[tokio::test]
    async fn test_range_functionality_combined_with_multiple_files() {
        let infra = Arc::new(MockCompositeService::new());

        infra.add_file(PathBuf::from("/test/file_a.txt"), "A1\nA2\nA3".to_string());
        infra.add_file(
            PathBuf::from("/test/file_b.txt"),
            "B1\nB2\nB3\nB4".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test multiple files with different ranges
        let url = "Check @[/test/file_a.txt:1:2] and @[/test/file_b.txt:3:4]";
        let attachments = chat_request.attachments(&url).await.unwrap();

        assert_eq!(attachments.len(), 2);
        assert_eq!(
            attachments[0].content,
            AttachmentContent::FileContent {
                content: "A1\nA2".to_string(),
                start_line: 1,
                end_line: 2,
                total_lines: 3,
            }
        );
        assert_eq!(
            attachments[1].content,
            AttachmentContent::FileContent {
                content: "B3\nB4".to_string(),
                start_line: 3,
                end_line: 4,
                total_lines: 4,
            }
        );
    }

    #[tokio::test]
    async fn test_range_functionality_preserves_metadata() {
        let infra = Arc::new(MockCompositeService::new());

        infra.add_file(
            PathBuf::from("/test/metadata_test.txt"),
            "Meta1\nMeta2\nMeta3\nMeta4\nMeta5\nMeta6\nMeta7".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test that metadata is preserved correctly with ranges
        let url = "@[/test/metadata_test.txt:3:5]";
        let attachments = chat_request.attachments(&url).await.unwrap();

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].path, "/test/metadata_test.txt");
        assert_eq!(
            attachments[0].content,
            AttachmentContent::FileContent {
                content: "Meta3\nMeta4\nMeta5".to_string(),
                start_line: 3,
                end_line: 5,
                total_lines: 7,
            }
        );
    }

    #[tokio::test]
    async fn test_range_functionality_vs_full_file() {
        let infra = Arc::new(MockCompositeService::new());

        infra.add_file(
            PathBuf::from("/test/comparison.txt"),
            "Full1\nFull2\nFull3\nFull4\nFull5".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test full file vs ranged file to ensure they're different
        let url_full = "@[/test/comparison.txt]";
        let url_range = "@[/test/comparison.txt:2:4]";
        let url_range_start = "@[/test/comparison.txt:2]";

        let attachments_full = chat_request.attachments(&url_full).await.unwrap();
        let attachments_range = chat_request.attachments(&url_range).await.unwrap();
        let attachments_range_start = chat_request.attachments(&url_range_start).await.unwrap();

        assert_eq!(attachments_full.len(), 1);
        assert_eq!(
            attachments_full[0].content,
            AttachmentContent::FileContent {
                content: "Full1\nFull2\nFull3\nFull4\nFull5".to_string(),
                start_line: 1,
                end_line: 5,
                total_lines: 5,
            }
        );

        assert_eq!(attachments_range.len(), 1);
        assert_eq!(
            attachments_range[0].content,
            AttachmentContent::FileContent {
                content: "Full2\nFull3\nFull4".to_string(),
                start_line: 2,
                end_line: 4,
                total_lines: 5,
            }
        );

        assert_eq!(attachments_range_start.len(), 1);
        assert_eq!(
            attachments_range_start[0].content,
            AttachmentContent::FileContent {
                content: "Full2\nFull3\nFull4\nFull5".to_string(),
                start_line: 2,
                end_line: 5,
                total_lines: 5,
            }
        );
    }
}
