use forge_app::{EnvironmentService, Infrastructure};

use crate::embedding::OpenAIEmbeddingService;
use crate::env::ForgeEnvironmentService;
use crate::file_read::ForgeFileReadService;
use crate::file_write::ForgeFileWriteService;
use crate::qdrant::QdrantVectorIndex;

pub struct ForgeInfra {
    file_read_service: ForgeFileReadService,
    environment_service: ForgeEnvironmentService,
    information_repo: QdrantVectorIndex,
    embedding_service: OpenAIEmbeddingService,
    file_write_service: ForgeFileWriteService,
}

impl ForgeInfra {
    pub fn new(restricted: bool) -> Self {
        let _environment_service = ForgeEnvironmentService::new(restricted);
        let env = _environment_service.get_environment();
        Self {
            file_read_service: ForgeFileReadService::new(),
            environment_service: _environment_service,
            information_repo: QdrantVectorIndex::new(env.clone(), "user_feedback"),
            embedding_service: OpenAIEmbeddingService::new(env),
            file_write_service: ForgeFileWriteService::new(),
        }
    }
}

impl Infrastructure for ForgeInfra {
    type EnvironmentService = ForgeEnvironmentService;
    type FileReadService = ForgeFileReadService;
    type FileWriteService = ForgeFileWriteService;
    type VectorIndex = QdrantVectorIndex;
    type EmbeddingService = OpenAIEmbeddingService;

    fn environment_service(&self) -> &Self::EnvironmentService {
        &self.environment_service
    }

    fn file_read_service(&self) -> &Self::FileReadService {
        &self.file_read_service
    }

    fn file_write_service(&self) -> &Self::FileWriteService {
        &self.file_write_service
    }

    fn vector_index(&self) -> &Self::VectorIndex {
        &self.information_repo
    }

    fn embedding_service(&self) -> &Self::EmbeddingService {
        &self.embedding_service
    }
}
