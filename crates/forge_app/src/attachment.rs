use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use base64::Engine;
use forge_domain::{Attachment, AttachmentService, ContentType, ImageType};

use crate::{FileReadService, Infrastructure};
// TODO: bring pdf support, pdf is just a collection of images.

pub struct ForgeChatRequest<F> {
    infra: Arc<F>,
}

impl<F: Infrastructure> ForgeChatRequest<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    async fn prepare_attachments<T: AsRef<Path>>(&self, paths: HashSet<T>) -> HashSet<Attachment> {
        futures::future::join_all(
            paths
                .into_iter()
                .map(|v| v.as_ref().to_path_buf())
                .map(|v| self.populate_attachments(v)),
        )
        .await
        .into_iter()
        .filter_map(|v| v.ok())
        .collect::<HashSet<_>>()
    }

    fn prepare_message(
        &self,
        mut message: String,
        attachments: &mut HashSet<Attachment>,
    ) -> String {
        for attachment in attachments.clone() {
            if let ContentType::Text = &attachment.content_type {
                let xml = format!(
                    "<file path=\"{}\">{}</file>",
                    attachment.path, attachment.content
                );
                message.push_str(&xml);

                attachments.remove(&attachment);
            }
        }

        message
    }
    async fn populate_attachments(&self, v: PathBuf) -> anyhow::Result<Attachment> {
        let path = v.to_string_lossy().to_string();
        let ext = v.extension().map(|v| v.to_string_lossy().to_string());
        let read = self.infra.file_read_service().read(v.as_path()).await?;
        if let Some(extension) = ext.as_ref().and_then(|v| ImageType::from_str(v).ok()) {
            Ok(Attachment {
                content: base64::engine::general_purpose::STANDARD.encode(read),
                path,
                content_type: ContentType::Image(extension),
            })
        } else {
            Ok(Attachment {
                content: String::from_utf8(read.to_vec())?,
                path,
                content_type: ContentType::Text,
            })
        }
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> AttachmentService for ForgeChatRequest<F> {
    async fn attachments(&self, chat: String) -> anyhow::Result<(String, HashSet<Attachment>)> {
        let mut attachments = self.prepare_attachments(Attachment::parse_all(&chat)).await;

        Ok((self.prepare_message(chat, &mut attachments), attachments))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use base64::Engine; // Add the Engine trait
    use bytes::Bytes;
    use forge_domain::{AttachmentService, ContentType, ImageType};

    use crate::attachment::ForgeChatRequest;
    use crate::{
        EmbeddingService, EnvironmentService, FileReadService, Infrastructure, VectorIndex,
    };
    use forge_domain::{Environment, Point, Query, Suggestion};

    struct MockEnvironmentService {}

    #[async_trait::async_trait]
    impl EnvironmentService for MockEnvironmentService {
        fn get_environment(&self) -> Environment {
            Environment {
                os: "test".to_string(),
                pid: 12345,
                cwd: PathBuf::from("/test"),
                home: Some(PathBuf::from("/home/test")),
                shell: "bash".to_string(),
                qdrant_key: None,
                qdrant_cluster: None,
                base_path: PathBuf::from("/base"),
                provider_key: "key".to_string(),
                provider_url: "url".to_string(),
                openai_key: None,
            }
        }
    }

    struct MockFileReadService {
        files: Mutex<HashMap<PathBuf, String>>,
    }

    impl MockFileReadService {
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

            Self { files: Mutex::new(files) }
        }

        fn add_file(&self, path: PathBuf, content: String) {
            let mut files = self.files.lock().unwrap();
            files.insert(path, content);
        }
    }

    #[async_trait::async_trait]
    impl FileReadService for MockFileReadService {
        async fn read(&self, path: &Path) -> anyhow::Result<Bytes> {
            let files = self.files.lock().unwrap();
            match files.get(path) {
                Some(content) => Ok(Bytes::from(content.clone())),
                None => Err(anyhow::anyhow!("File not found: {:?}", path)),
            }
        }
    }

    struct MockVectorIndex {}

    #[async_trait::async_trait]
    impl VectorIndex<Suggestion> for MockVectorIndex {
        async fn store(&self, _point: Point<Suggestion>) -> anyhow::Result<()> {
            Ok(())
        }

        async fn search(&self, _query: Query) -> anyhow::Result<Vec<Point<Suggestion>>> {
            Ok(vec![])
        }
    }

    struct MockEmbeddingService {}

    #[async_trait::async_trait]
    impl EmbeddingService for MockEmbeddingService {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }
    }

    struct MockInfrastructure {
        env_service: MockEnvironmentService,
        file_service: MockFileReadService,
        vector_index: MockVectorIndex,
        embedding_service: MockEmbeddingService,
    }

    impl MockInfrastructure {
        fn new() -> Self {
            Self {
                env_service: MockEnvironmentService {},
                file_service: MockFileReadService::new(),
                vector_index: MockVectorIndex {},
                embedding_service: MockEmbeddingService {},
            }
        }
    }

    impl Infrastructure for MockInfrastructure {
        type EnvironmentService = MockEnvironmentService;
        type FileReadService = MockFileReadService;
        type VectorIndex = MockVectorIndex;
        type EmbeddingService = MockEmbeddingService;

        fn environment_service(&self) -> &Self::EnvironmentService {
            &self.env_service
        }

        fn file_read_service(&self) -> &Self::FileReadService {
            &self.file_service
        }

        fn vector_index(&self) -> &Self::VectorIndex {
            &self.vector_index
        }

        fn embedding_service(&self) -> &Self::EmbeddingService {
            &self.embedding_service
        }
    }

    #[tokio::test]
    async fn test_attachments_function_with_text_file() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with a text file path in chat message
        let chat_message = "Check this file @/test/file1.txt please".to_string();

        // Execute
        let (result_message, attachments) = chat_request
            .attachments(chat_message.clone())
            .await
            .unwrap();

        // Assert
        // The text file should be removed from attachments and added to the message
        assert_eq!(attachments.len(), 0);
        assert!(result_message
            .contains("<file path=\"/test/file1.txt\">This is a text file content</file>"));
    }

    #[tokio::test]
    async fn test_attachments_function_with_image() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with an image file
        let chat_message = "Look at this image @/test/image.png".to_string();

        // Execute
        let (result_message, attachments) = chat_request
            .attachments(chat_message.clone())
            .await
            .unwrap();

        // Assert
        // The image should remain in attachments and not be added to the message
        assert_eq!(attachments.len(), 1);
        assert_eq!(result_message, chat_message);

        let attachment = attachments.iter().next().unwrap();
        assert_eq!(attachment.path, "/test/image.png");
        assert!(matches!(
            attachment.content_type,
            ContentType::Image(ImageType::Png)
        ));

        // Base64 content should be the encoded mock binary content
        let expected_base64 =
            base64::engine::general_purpose::STANDARD.encode("mock-binary-content");
        assert_eq!(attachment.content, expected_base64);
    }

    #[tokio::test]
    async fn test_attachments_function_with_jpg_image_with_spaces() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with an image file that has spaces in the path
        let chat_message = "Look at this image @\"/test/image with spaces.jpg\"".to_string();

        // Execute
        let (result_message, attachments) = chat_request
            .attachments(chat_message.clone())
            .await
            .unwrap();

        // Assert
        // The image should remain in attachments
        assert_eq!(attachments.len(), 1);
        assert_eq!(result_message, chat_message);

        let attachment = attachments.iter().next().unwrap();
        assert_eq!(attachment.path, "/test/image with spaces.jpg");
        assert!(matches!(
            attachment.content_type,
            ContentType::Image(ImageType::Jpeg)
        ));

        // Base64 content should be the encoded mock jpeg content
        let expected_base64 = base64::engine::general_purpose::STANDARD.encode("mock-jpeg-content");
        assert_eq!(attachment.content, expected_base64);
    }

    #[tokio::test]
    async fn test_attachments_function_with_multiple_files() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());

        // Add an extra file to our mock service
        infra.file_service.add_file(
            PathBuf::from("/test/file2.txt"),
            "This is another text file".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with multiple files mentioned
        let chat_message = "Check these files: @/test/file1.txt and @/test/file2.txt and this image @/test/image.png".to_string();

        // Execute
        let (result_message, attachments) = chat_request
            .attachments(chat_message.clone())
            .await
            .unwrap();

        // Assert
        // The text files should be removed from attachments and added to the message
        // Only the image should remain in attachments
        assert_eq!(attachments.len(), 1);
        assert!(matches!(
            attachments.iter().next().unwrap().content_type,
            ContentType::Image(_)
        ));

        assert!(result_message
            .contains("<file path=\"/test/file1.txt\">This is a text file content</file>"));
        assert!(result_message
            .contains("<file path=\"/test/file2.txt\">This is another text file</file>"));
    }

    #[tokio::test]
    async fn test_attachments_function_with_nonexistent_file() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with a file that doesn't exist
        let chat_message = "Check this file @/test/nonexistent.txt".to_string();

        // Execute
        let (result_message, attachments) = chat_request
            .attachments(chat_message.clone())
            .await
            .unwrap();

        // Assert - nonexistent files should be ignored
        assert_eq!(attachments.len(), 0);
        assert_eq!(result_message, chat_message);
    }

    #[tokio::test]
    async fn test_attachments_function_empty_message() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with an empty message
        let chat_message = "".to_string();

        // Execute
        let (result_message, attachments) = chat_request
            .attachments(chat_message.clone())
            .await
            .unwrap();

        // Assert - no attachments
        assert_eq!(attachments.len(), 0);
        assert_eq!(result_message, "");
    }

    #[tokio::test]
    async fn test_attachments_function_with_unsupported_image_extension() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());

        // Add a file with unsupported extension
        infra.file_service.add_file(
            PathBuf::from("/test/unknown.xyz"),
            "Some content".to_string(),
        );

        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with the file
        let chat_message = "Check this file @/test/unknown.xyz".to_string();

        // Execute
        let (result_message, attachments) = chat_request
            .attachments(chat_message.clone())
            .await
            .unwrap();

        // Assert - should be treated as text
        assert_eq!(attachments.len(), 0);
        assert!(result_message.contains("<file path=\"/test/unknown.xyz\">Some content</file>"));
    }
}
