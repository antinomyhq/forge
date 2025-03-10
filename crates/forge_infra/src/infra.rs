use std::sync::Arc;

use forge_app::{EnvironmentService, Infrastructure};

use crate::create_dirs::ForgeCreateDirsService;
use crate::embedding::OpenAIEmbeddingService;
use crate::env::ForgeEnvironmentService;
use crate::file_meta::ForgeFileMetaService;
use crate::file_read::ForgeFileReadService;
use crate::file_remove::ForgeFileRemoveService;
use crate::file_snap::ForgeFileSnapshotService;
use crate::file_write::ForgeFileWriteService;
use crate::qdrant::QdrantVectorIndex;

pub struct ForgeInfra {
    file_read_service: ForgeFileReadService,
    file_write_service: ForgeFileWriteService<ForgeFileSnapshotService>,
    environment_service: ForgeEnvironmentService,
    information_repo: QdrantVectorIndex,
    embedding_service: OpenAIEmbeddingService,
    file_snapshot_service: Arc<ForgeFileSnapshotService>,
    file_meta_service: ForgeFileMetaService,
    file_remove_service: ForgeFileRemoveService<ForgeFileSnapshotService>,
    create_dirs_service: ForgeCreateDirsService,
}

impl ForgeInfra {
    pub fn new(restricted: bool) -> Self {
        let environment_service = ForgeEnvironmentService::new(restricted);
        let env = environment_service.get_environment();
        let file_snapshot_service = Arc::new(ForgeFileSnapshotService::new(env.clone()));
        Self {
            file_read_service: ForgeFileReadService::new(),
            file_write_service: ForgeFileWriteService::new(file_snapshot_service.clone()),
            file_meta_service: ForgeFileMetaService,
            file_remove_service: ForgeFileRemoveService::new(file_snapshot_service.clone()),
            environment_service,
            information_repo: QdrantVectorIndex::new(env.clone(), "user_feedback"),
            embedding_service: OpenAIEmbeddingService::new(env.clone()),
            file_snapshot_service,
            create_dirs_service: ForgeCreateDirsService,
        }
    }
}

impl Infrastructure for ForgeInfra {
    type EnvironmentService = ForgeEnvironmentService;
    type FileReadService = ForgeFileReadService;
    type FileWriteService = ForgeFileWriteService<ForgeFileSnapshotService>;
    type VectorIndex = QdrantVectorIndex;
    type EmbeddingService = OpenAIEmbeddingService;
    type FileMetaService = ForgeFileMetaService;
    type FileSnapshotService = ForgeFileSnapshotService;
    type FileRemoveService = ForgeFileRemoveService<ForgeFileSnapshotService>;
    type CreateDirsService = ForgeCreateDirsService;

    fn environment_service(&self) -> &Self::EnvironmentService {
        &self.environment_service
    }

    fn file_read_service(&self) -> &Self::FileReadService {
        &self.file_read_service
    }

    fn vector_index(&self) -> &Self::VectorIndex {
        &self.information_repo
    }

    fn embedding_service(&self) -> &Self::EmbeddingService {
        &self.embedding_service
    }

    fn file_write_service(&self) -> &Self::FileWriteService {
        &self.file_write_service
    }

    fn file_meta_service(&self) -> &Self::FileMetaService {
        &self.file_meta_service
    }

    fn file_snapshot_service(&self) -> &Self::FileSnapshotService {
        &self.file_snapshot_service
    }

    fn file_remove_service(&self) -> &Self::FileRemoveService {
        &self.file_remove_service
    }

    fn create_dirs_service(&self) -> &Self::CreateDirsService {
        &self.create_dirs_service
    }
}
