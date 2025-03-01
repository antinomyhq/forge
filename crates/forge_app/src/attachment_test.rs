#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use base64::Engine;  // Add the Engine trait
    use forge_domain::{AttachmentService, ContentType, ImageType};

    use crate::{EmbeddingService, EnvironmentService, FileReadService, Infrastructure, VectorIndex};
    use crate::attachment::ForgeChatRequest;
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
            
            Self {
                files: Mutex::new(files),
            }
        }
        
        fn add_file(&self, path: PathBuf, content: String) {
            let mut files = self.files.lock().unwrap();
            files.insert(path, content);
        }
    }
    
    #[async_trait::async_trait]
    impl FileReadService for MockFileReadService {
        async fn read(&self, path: &Path) -> anyhow::Result<String> {
            let files = self.files.lock().unwrap();
            match files.get(path) {
                Some(content) => Ok(content.clone()),
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
        let (result_message, attachments) = chat_request.attachments(chat_message.clone()).await.unwrap();
        
        // Assert
        // The text file should be removed from attachments and added to the message
        assert_eq!(attachments.len(), 0);
        assert!(result_message.contains("<file path=\"/test/file1.txt\">This is a text file content</file>"));
    }

    #[tokio::test]
    async fn test_attachments_function_with_image() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
        let chat_request = ForgeChatRequest::new(infra.clone());
        
        // Test with an image file
        let chat_message = "Look at this image @/test/image.png".to_string();
        
        // Execute
        let (result_message, attachments) = chat_request.attachments(chat_message.clone()).await.unwrap();
        
        // Assert
        // The image should remain in attachments and not be added to the message
        assert_eq!(attachments.len(), 1);
        assert_eq!(result_message, chat_message);
        
        let attachment = attachments.iter().next().unwrap();
        assert_eq!(attachment.path, "/test/image.png");
        assert!(matches!(attachment.content_type, ContentType::Image(ImageType::Png)));
        
        // Base64 content should be the encoded mock binary content
        let expected_base64 = base64::engine::general_purpose::STANDARD.encode("mock-binary-content");
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
        let (result_message, attachments) = chat_request.attachments(chat_message.clone()).await.unwrap();
        
        // Assert
        // The image should remain in attachments 
        assert_eq!(attachments.len(), 1);
        assert_eq!(result_message, chat_message);
        
        let attachment = attachments.iter().next().unwrap();
        assert_eq!(attachment.path, "/test/image with spaces.jpg");
        assert!(matches!(attachment.content_type, ContentType::Image(ImageType::Jpeg)));
        
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
            "This is another text file".to_string()
        );
        
        let chat_request = ForgeChatRequest::new(infra.clone());
        
        // Test with multiple files mentioned
        let chat_message = "Check these files: @/test/file1.txt and @/test/file2.txt and this image @/test/image.png".to_string();
        
        // Execute
        let (result_message, attachments) = chat_request.attachments(chat_message.clone()).await.unwrap();
        
        // Assert
        // The text files should be removed from attachments and added to the message
        // Only the image should remain in attachments
        assert_eq!(attachments.len(), 1);
        assert!(matches!(attachments.iter().next().unwrap().content_type, ContentType::Image(_)));
        
        assert!(result_message.contains("<file path=\"/test/file1.txt\">This is a text file content</file>"));
        assert!(result_message.contains("<file path=\"/test/file2.txt\">This is another text file</file>"));
    }

    #[tokio::test]
    async fn test_attachments_function_with_nonexistent_file() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
        let chat_request = ForgeChatRequest::new(infra.clone());
        
        // Test with a file that doesn't exist
        let chat_message = "Check this file @/test/nonexistent.txt".to_string();
        
        // Execute
        let (result_message, attachments) = chat_request.attachments(chat_message.clone()).await.unwrap();
        
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
        let (result_message, attachments) = chat_request.attachments(chat_message.clone()).await.unwrap();
        
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
            "Some content".to_string()
        );
        
        let chat_request = ForgeChatRequest::new(infra.clone());
        
        // Test with the file
        let chat_message = "Check this file @/test/unknown.xyz".to_string();
        
        // Execute
        let (result_message, attachments) = chat_request.attachments(chat_message.clone()).await.unwrap();
        
        // Assert - should be treated as text
        assert_eq!(attachments.len(), 0);
        assert!(result_message.contains("<file path=\"/test/unknown.xyz\">Some content</file>"));
    }
}