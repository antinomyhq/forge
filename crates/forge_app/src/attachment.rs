use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use forge_domain::{Attachment, AttachmentService, ContentType};

use crate::{FileReadService, Infrastructure};
// TODO: bring pdf support, pdf is just a collection of images.

pub struct ForgeChatRequest<F> {
    infra: Arc<F>,
}

impl<F: Infrastructure> ForgeChatRequest<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    async fn prepare_attachments<T: AsRef<Path>>(&self, paths: HashSet<T>) -> Vec<Attachment> {
        futures::future::join_all(
            paths
                .into_iter()
                .map(|v| v.as_ref().to_path_buf())
                .map(|v| self.populate_attachments(v)),
        )
        .await
        .into_iter()
        .filter_map(|v| v.ok())
        .collect::<Vec<_>>()
    }

    async fn populate_attachments(&self, path: PathBuf) -> anyhow::Result<Attachment> {
        let extension = path.extension().map(|v| v.to_string_lossy().to_string());
        let read = self.infra.file_read_service().read(path.as_path()).await?;
        let path = path.to_string_lossy().to_string();
        if let Some(img_extension) = extension.and_then(|ext| match ext.as_str() {
            "jpeg" | "jpg" => Some("jpeg"),
            "png" => Some("png"),
            "webp" => Some("webp"),
            _ => None,
        }) {
            let base_64_encoded = base64::engine::general_purpose::STANDARD.encode(read);
            let content = format!("data:image/{};base64,{}", img_extension, base_64_encoded);
            Ok(Attachment { content, path, content_type: ContentType::Image })
        } else {
            let content = String::from_utf8(read.to_vec())?;
            Ok(Attachment { content, path, content_type: ContentType::Text })
        }
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> AttachmentService for ForgeChatRequest<F> {
    async fn attachments(&self, url: &str) -> anyhow::Result<Vec<Attachment>> {
        let attachments = self.prepare_attachments(Attachment::parse_all(url)).await;
        Ok(attachments)
    }
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use base64::Engine;
    use bytes::Bytes;
    use forge_domain::{AttachmentService, ContentType, Environment, Point, Query, Suggestion};
    use forge_snaps::{FileSnapshotService, SnapshotInfo, SnapshotMetadata};

    use crate::attachment::ForgeChatRequest;
    use crate::{
        EmbeddingService, EnvironmentService, FileExist, FileReadService, FileWriteService,
        Infrastructure, VectorIndex,
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
                qdrant_key: None,
                qdrant_cluster: None,
                base_path: PathBuf::from("/base"),
                provider_key: "key".to_string(),
                provider_url: "url".to_string(),
                openai_key: None,
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
    impl FileReadService for MockFileService {
        async fn read(&self, path: &Path) -> anyhow::Result<Bytes> {
            let files = self.files.lock().unwrap();
            match files.iter().find(|v| v.0 == path) {
                Some((_, content)) => Ok(content.clone()),
                None => Err(anyhow::anyhow!("File not found: {:?}", path)),
            }
        }
    }
    #[derive(Debug)]
    pub struct MockVectorIndex {}

    #[async_trait::async_trait]
    impl VectorIndex<Suggestion> for MockVectorIndex {
        async fn store(&self, _point: Point<Suggestion>) -> anyhow::Result<()> {
            Ok(())
        }

        async fn search(&self, _query: Query) -> anyhow::Result<Vec<Point<Suggestion>>> {
            Ok(vec![])
        }
    }
    #[derive(Debug)]
    pub struct MockEmbeddingService {}

    #[async_trait::async_trait]
    impl EmbeddingService for MockEmbeddingService {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }
    }

    #[derive(Debug)]
    pub struct MockInfrastructure {
        env_service: MockEnvironmentService,
        file_service: MockFileService,
        vector_index: MockVectorIndex,
        embedding_service: MockEmbeddingService,
        file_snapshot_service: MockSnapService,
    }

    impl MockInfrastructure {
        pub fn new() -> Self {
            Self {
                env_service: MockEnvironmentService {},
                file_service: MockFileService::new(),
                vector_index: MockVectorIndex {},
                embedding_service: MockEmbeddingService {},
                file_snapshot_service: MockSnapService,
            }
        }
    }

    #[derive(Debug)]
    pub struct MockFileService {
        files: Mutex<Vec<(PathBuf, Bytes)>>,
    }

    #[async_trait::async_trait]
    impl FileWriteService for MockFileService {
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
    }

    #[derive(Debug)]
    pub struct MockSnapService;

    #[async_trait::async_trait]
    impl FileSnapshotService for MockSnapService {
        fn snapshot_dir(&self) -> PathBuf {
            todo!()
        }

        async fn create_snapshot(&self, _: &Path) -> anyhow::Result<SnapshotInfo> {
            todo!()
        }

        async fn list_snapshots(&self, _: &Path) -> anyhow::Result<Vec<SnapshotInfo>> {
            todo!()
        }

        async fn restore_by_timestamp(&self, _: &Path, _: u64) -> anyhow::Result<()> {
            todo!()
        }

        async fn restore_by_index(&self, _: &Path, _: isize) -> anyhow::Result<()> {
            todo!()
        }

        async fn restore_previous(&self, _: &Path) -> anyhow::Result<()> {
            todo!()
        }

        async fn get_snapshot_by_timestamp(
            &self,
            _: &Path,
            _: u64,
        ) -> anyhow::Result<SnapshotMetadata> {
            todo!()
        }

        async fn get_snapshot_by_index(
            &self,
            _: &Path,
            _: isize,
        ) -> anyhow::Result<SnapshotMetadata> {
            todo!()
        }

        async fn purge_older_than(&self, _: u32) -> anyhow::Result<usize> {
            todo!()
        }
    }

    #[async_trait::async_trait]
    impl FileExist for MockFileService {
        async fn exist(&self, path: &Path) -> anyhow::Result<bool> {
            Ok(self.files.lock().unwrap().iter().any(|(p, _)| p == path))
        }
    }

    impl Infrastructure for MockInfrastructure {
        type EnvironmentService = MockEnvironmentService;
        type FileReadService = MockFileService;
        type FileWriteService = MockFileService;
        type VectorIndex = MockVectorIndex;
        type EmbeddingService = MockEmbeddingService;
        type FileSnapshotService = MockSnapService;
        type FileExist = MockFileService;

        fn environment_service(&self) -> &Self::EnvironmentService {
            &self.env_service
        }

        fn file_read_service(&self) -> &Self::FileReadService {
            &self.file_service
        }

        fn file_write_service(&self) -> &Self::FileWriteService {
            &self.file_service
        }

        fn vector_index(&self) -> &Self::VectorIndex {
            &self.vector_index
        }

        fn embedding_service(&self) -> &Self::EmbeddingService {
            &self.embedding_service
        }

        fn file_snapshot_service(&self) -> &Self::FileSnapshotService {
            &self.file_snapshot_service
        }

        fn file_exist_service(&self) -> &Self::FileExist {
            &self.file_service
        }
    }

    #[tokio::test]
    async fn test_add_url_with_text_file() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with a text file path in chat message
        let url = "@/test/file1.txt".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert
        // Text files should be included in the attachments
        assert_eq!(attachments.len(), 1);
        let attachment = attachments.first().unwrap();
        assert_eq!(attachment.path, "/test/file1.txt");
        assert_eq!(attachment.content_type, ContentType::Text);
        assert_eq!(attachment.content, "This is a text file content");
    }

    #[tokio::test]
    async fn test_add_url_with_image() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with an image file
        let url = "@/test/image.png".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert
        assert_eq!(attachments.len(), 1);
        let attachment = attachments.first().unwrap();
        assert_eq!(attachment.path, "/test/image.png");
        assert!(matches!(attachment.content_type, ContentType::Image));

        // Base64 content should be the encoded mock binary content with proper data URI
        // format
        let expected_base64 =
            base64::engine::general_purpose::STANDARD.encode("mock-binary-content");
        assert_eq!(
            attachment.content,
            format!("data:image/png;base64,{}", expected_base64)
        );
    }

    #[tokio::test]
    async fn test_add_url_with_jpg_image_with_spaces() {
        // Setup
        let infra = Arc::new(MockInfrastructure::new());
        let chat_request = ForgeChatRequest::new(infra.clone());

        // Test with an image file that has spaces in the path
        let url = "@\"/test/image with spaces.jpg\"".to_string();

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
            attachment.content,
            format!("data:image/jpeg;base64,{}", expected_base64)
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
        let url = "@/test/file1.txt @/test/file2.txt @/test/image.png".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert
        // All files should be included in the attachments
        assert_eq!(attachments.len(), 3);

        // Verify that each expected file is in the attachments
        let has_file1 = attachments
            .iter()
            .any(|a| a.path == "/test/file1.txt" && matches!(a.content_type, ContentType::Text));
        let has_file2 = attachments
            .iter()
            .any(|a| a.path == "/test/file2.txt" && matches!(a.content_type, ContentType::Text));
        let has_image = attachments
            .iter()
            .any(|a| a.path == "/test/image.png" && matches!(a.content_type, ContentType::Image));

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
        let url = "@/test/nonexistent.txt".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert - nonexistent files should be ignored
        assert_eq!(attachments.len(), 0);
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
        let url = "@/test/unknown.xyz".to_string();

        // Execute
        let attachments = chat_request.attachments(&url).await.unwrap();

        // Assert - should be treated as text
        assert_eq!(attachments.len(), 1);
        let attachment = attachments.first().unwrap();
        assert_eq!(attachment.path, "/test/unknown.xyz");
        assert_eq!(attachment.content_type, ContentType::Text);
        assert_eq!(attachment.content, "Some content");
    }
}
