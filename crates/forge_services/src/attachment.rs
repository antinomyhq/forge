use std::collections::HashSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_domain::{Attachment, AttachmentContent, AttachmentService, EnvironmentService, Image};

use crate::{FsReadService, Infrastructure};

#[derive(Clone)]

pub struct ForgeChatRequest<F> {
    infra: Arc<F>,
}

impl<F: Infrastructure> ForgeChatRequest<F> {
    async fn generate_image_content(
        path: &Path,
        img_format: String,
        infra: &impl FsReadService,
    ) -> anyhow::Result<Image> {
        let bytes = infra.read(path).await?;

        Ok(Image::new_bytes(bytes, img_format))
    }

    async fn generate_text_content(
        path: &Path,
        infra: &impl FsReadService,
    ) -> anyhow::Result<String> {
        const MAX_CHARS: u64 = 40_000;
        let (content, file_info) = infra.range_read_utf8(path, 0, MAX_CHARS).await?;
        let mut response = String::new();
        writeln!(response, "---")?;
        writeln!(response, "path: {}", path.display())?;

        writeln!(response, "start_char: {}", file_info.start_char)?;
        writeln!(response, "end_char: {}", file_info.end_char)?;
        writeln!(response, "total_chars: {}", file_info.total_chars)?;

        writeln!(response, "---")?;

        writeln!(response, "{}", &content)?;

        Ok(response)
    }

    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    async fn prepare_attachments<T: AsRef<Path>>(
        &self,
        paths: HashSet<T>,
    ) -> anyhow::Result<Vec<Attachment>> {
        futures::future::join_all(
            paths
                .into_iter()
                .map(|v| v.as_ref().to_path_buf())
                .map(|v| self.populate_attachments(v)),
        )
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()
    }

    async fn populate_attachments(&self, mut path: PathBuf) -> anyhow::Result<Attachment> {
        let extension = path.extension().map(|v| v.to_string_lossy().to_string());

        if !path.is_absolute() {
            path = self
                .infra
                .environment_service()
                .get_environment()
                .cwd
                .join(path);
        }

        // Determine file type (text or image with format)
        let mime_type = extension.and_then(|ext| match ext.as_str() {
            "jpeg" | "jpg" => Some("image/jpeg".to_string()),
            "png" => Some("image/png".to_string()),
            "webp" => Some("image/webp".to_string()),
            _ => None,
        });

        let content = match mime_type {
            Some(mime_type) => AttachmentContent::Image(
                Self::generate_image_content(&path, mime_type, self.infra.file_read_service())
                    .await?,
            ),
            None => AttachmentContent::FileContent(
                Self::generate_text_content(&path, self.infra.file_read_service()).await?,
            ),
        };

        Ok(Attachment { content, path: path.to_string_lossy().to_string() })
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> AttachmentService for ForgeChatRequest<F> {
    async fn attachments(&self, url: &str) -> anyhow::Result<Vec<Attachment>> {
        self.prepare_attachments(Attachment::parse_all(url)).await
    }
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use base64::Engine;
    use bytes::Bytes;
    use forge_domain::{
        AttachmentContent, AttachmentService, CommandOutput, Environment, EnvironmentService,
        Provider, ToolDefinition, ToolName, ToolOutput,
    };
    use forge_snaps::Snapshot;
    use serde_json::Value;

    use crate::attachment::ForgeChatRequest;
    use crate::utils::AttachmentExtension;
    use crate::{
        CommandExecutorService, FileRemoveService, FsCreateDirsService, FsMetaService,
        FsReadService, FsSnapshotService, FsWriteService, Infrastructure, InquireService,
        McpClient, McpServer,
    };

    #[derive(Debug)]
    pub struct MockEnvironmentService {}

    #[async_trait::async_trait]
    impl EnvironmentService for MockEnvironmentService {
        fn get_environment(&self) -> Environment {
            Environment {
                os: "test".to_string(),
                pid: 12345,
                cwd: PathBuf::from("/test"),
                home: Some(PathBuf::from("/home/test")),
                shell: "bash".to_string(),
                base_path: PathBuf::from("/base"),
                provider: Provider::open_router("test-key"),
                retry_config: Default::default(),
                model_cache_ttl: 60 * 60,
            }
        }
    }

    impl MockFileService {
        fn new() -> Self {
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

            Self {
                files: Mutex::new(
                    files
                        .into_iter()
                        .map(|(a, b)| (a, Bytes::from(b)))
                        .collect::<Vec<_>>(),
                ),
            }
        }

        fn add_file(&self, path: PathBuf, content: String) {
            let mut files = self.files.lock().unwrap();
            files.push((path, Bytes::from_owner(content)));
        }
    }

    #[async_trait::async_trait]
    impl FsReadService for MockFileService {
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
            _start_char: u64,
            _end_char: u64,
        ) -> anyhow::Result<(String, forge_fs::FileInfo)> {
            // For tests, we'll just read the entire file and return it
            let content = self.read_utf8(path).await?;
            let total_chars = content.len() as u64;

            // Return the entire content for simplicity in tests
            Ok((
                content,
                forge_fs::FileInfo::new(0, total_chars, total_chars),
            ))
        }
    }

    #[derive(Debug, Clone)]
    pub struct MockInfrastructure {
        env_service: Arc<MockEnvironmentService>,
        file_service: Arc<MockFileService>,
        file_snapshot_service: Arc<MockSnapService>,
    }

    impl MockInfrastructure {
        pub fn new() -> Self {
            Self {
                env_service: Arc::new(MockEnvironmentService {}),
                file_service: Arc::new(MockFileService::new()),
                file_snapshot_service: Arc::new(MockSnapService),
            }
        }
    }

    #[derive(Debug)]
    pub struct MockFileService {
        files: Mutex<Vec<(PathBuf, Bytes)>>,
    }

    #[async_trait::async_trait]
    impl FileRemoveService for MockFileService {
        async fn remove(&self, path: &Path) -> anyhow::Result<()> {
            if !self.exists(path).await? {
                return Err(anyhow::anyhow!("File not found: {:?}", path));
            }
            self.files.lock().unwrap().retain(|(p, _)| p != path);
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl FsCreateDirsService for MockFileService {
        async fn create_dirs(&self, path: &Path) -> anyhow::Result<()> {
            self.files
                .lock()
                .unwrap()
                .push((path.to_path_buf(), Bytes::new()));
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl FsWriteService for MockFileService {
        async fn write(&self, path: &Path, contents: Bytes) -> anyhow::Result<()> {
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

            self.write(&path, content.to_string().into()).await?;

            Ok(path)
        }
    }

    #[derive(Debug)]
    pub struct MockSnapService;

    #[async_trait::async_trait]
    impl FsSnapshotService for MockSnapService {
        async fn create_snapshot(&self, _: &Path) -> anyhow::Result<Snapshot> {
            unimplemented!()
        }

        async fn undo_snapshot(&self, _: &Path) -> anyhow::Result<()> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FsMetaService for MockFileService {
        async fn is_file(&self, path: &Path) -> anyhow::Result<bool> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .filter(|v| v.0.extension().is_some())
                .any(|(p, _)| p == path))
        }

        async fn exists(&self, path: &Path) -> anyhow::Result<bool> {
            Ok(self.files.lock().unwrap().iter().any(|(p, _)| p == path))
        }
    }

    #[async_trait::async_trait]
    impl McpClient for () {
        async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
            Ok(vec![])
        }

        async fn call(&self, _: &ToolName, _: Value) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::default())
        }
    }

    #[async_trait::async_trait]
    impl McpServer for () {
        type Client = ();

        async fn connect(&self, _: forge_domain::McpServerConfig) -> anyhow::Result<Self::Client> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl CommandExecutorService for () {
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

        async fn execute_command_raw(&self, _: &str) -> anyhow::Result<std::process::ExitStatus> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl InquireService for () {
        /// Prompts the user with question
        async fn prompt_question(&self, question: &str) -> anyhow::Result<Option<String>> {
            // For testing, we can just return the question as the answer
            Ok(Some(question.to_string()))
        }

        /// Prompts the user to select a single option from a list
        async fn select_one(
            &self,
            _: &str,
            options: Vec<String>,
        ) -> anyhow::Result<Option<String>> {
            // For testing, we can just return the first option
            if options.is_empty() {
                return Err(anyhow::anyhow!("No options provided"));
            }
            Ok(Some(options[0].clone()))
        }

        /// Prompts the user to select multiple options from a list
        async fn select_many(
            &self,
            _: &str,
            options: Vec<String>,
        ) -> anyhow::Result<Option<Vec<String>>> {
            // For testing, we can just return all options
            if options.is_empty() {
                return Err(anyhow::anyhow!("No options provided"));
            }
            Ok(Some(options))
        }
    }

    impl Infrastructure for MockInfrastructure {
        type EnvironmentService = MockEnvironmentService;
        type FsReadService = MockFileService;
        type FsWriteService = MockFileService;
        type FsRemoveService = MockFileService;
        type FsMetaService = MockFileService;
        type FsCreateDirsService = MockFileService;
        type FsSnapshotService = MockSnapService;
        type CommandExecutorService = ();
        type InquireService = ();
        type McpServer = ();

        fn environment_service(&self) -> &Self::EnvironmentService {
            &self.env_service
        }

        fn file_read_service(&self) -> &Self::FsReadService {
            &self.file_service
        }

        fn file_write_service(&self) -> &Self::FsWriteService {
            &self.file_service
        }

        fn file_meta_service(&self) -> &Self::FsMetaService {
            &self.file_service
        }

        fn file_snapshot_service(&self) -> &Self::FsSnapshotService {
            &self.file_snapshot_service
        }

        fn file_remove_service(&self) -> &Self::FsRemoveService {
            &self.file_service
        }

        fn create_dirs_service(&self) -> &Self::FsCreateDirsService {
            &self.file_service
        }

        fn command_executor_service(&self) -> &Self::CommandExecutorService {
            &()
        }

        fn inquire_service(&self) -> &Self::InquireService {
            &()
        }

        fn mcp_server(&self) -> &Self::McpServer {
            &()
        }
    }

    #[tokio::test]
    async fn test_add_url_with_text_file() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
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
        assert!(attachment.content.contains("start_char:"));
        assert!(attachment.content.contains("end_char:"));
        assert!(attachment.content.contains("total_chars:"));
    }

    #[tokio::test]
    async fn test_add_url_with_image() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
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
        let infra = Arc::new(MockInfrastructure::new());
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
        let infra = Arc::new(MockInfrastructure::new());

        // Add an extra file to our mock service
        infra.file_service.add_file(
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
            a.path == "/test/file1.txt" && matches!(a.content, AttachmentContent::FileContent(_))
        });
        let has_file2 = attachments.iter().any(|a| {
            a.path == "/test/file2.txt" && matches!(a.content, AttachmentContent::FileContent(_))
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
        let infra = Arc::new(MockInfrastructure::new());
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
        let infra = Arc::new(MockInfrastructure::new());
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
        let infra = Arc::new(MockInfrastructure::new());

        // Add a file with unsupported extension
        infra.file_service.add_file(
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
        assert!(attachment.content.contains("start_char:"));
        assert!(attachment.content.contains("end_char:"));
        assert!(attachment.content.contains("total_chars:"));
    }
}
