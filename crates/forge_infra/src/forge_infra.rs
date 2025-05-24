use std::sync::Arc;

use forge_domain::EnvironmentService;
use forge_services::Infrastructure;

use crate::dir_create::ForgeDirCreateService;
use crate::env::ForgeEnvironmentService;
use crate::executor::ForgeCommandExecutorService;
use crate::fs_meta::ForgeFileMetaService;
use crate::fs_read::ForgeFileReadService;
use crate::fs_remove::ForgeFileRemoveService;
use crate::fs_snap::ForgeFileSnapshotService;
use crate::fs_undo::ForgeFileUndoService;
use crate::fs_write::ForgeFileWriteService;
use crate::mcp_server::ForgeMcpServer;

#[derive(Clone)]
pub struct ForgeInfra {
    file_read_service: Arc<ForgeFileReadService>,
    file_write_service: Arc<ForgeFileWriteService<ForgeFileSnapshotService>>,
    environment_service: Arc<ForgeEnvironmentService>,
    file_snapshot_service: Arc<ForgeFileSnapshotService>,
    file_meta_service: Arc<ForgeFileMetaService>,
    file_remove_service: Arc<ForgeFileRemoveService<ForgeFileSnapshotService>>,
    file_undo_service: Arc<ForgeFileUndoService>,
    dir_create_service: Arc<ForgeDirCreateService>,
    command_executor_service: Arc<ForgeCommandExecutorService>,
    mcp_server: ForgeMcpServer,
}

impl ForgeInfra {
    pub fn new(restricted: bool) -> Self {
        let environment_service = Arc::new(ForgeEnvironmentService::new(restricted));
        let env = environment_service.get_environment();
        let file_snapshot_service = Arc::new(ForgeFileSnapshotService::new(env.clone()));
        Self {
            file_read_service: Arc::new(ForgeFileReadService::new()),
            file_write_service: Arc::new(ForgeFileWriteService::new(file_snapshot_service.clone())),
            file_meta_service: Arc::new(ForgeFileMetaService),
            file_remove_service: Arc::new(ForgeFileRemoveService::new(
                file_snapshot_service.clone(),
            )),
            file_undo_service: Arc::new(ForgeFileUndoService::with_env(env.clone())),
            environment_service,
            file_snapshot_service,
            dir_create_service: Arc::new(ForgeDirCreateService),
            command_executor_service: Arc::new(ForgeCommandExecutorService::new(
                restricted,
                env.clone(),
            )),
            mcp_server: ForgeMcpServer,
        }
    }
}

impl Infrastructure for ForgeInfra {
    type EnvironmentService = ForgeEnvironmentService;
    type FsReadService = ForgeFileReadService;
    type FsWriteService = ForgeFileWriteService<ForgeFileSnapshotService>;
    type FsMetaService = ForgeFileMetaService;
    type FsSnapshotService = ForgeFileSnapshotService;
    type FsRemoveService = ForgeFileRemoveService<ForgeFileSnapshotService>;
    type FsUndoService = ForgeFileUndoService;
    type DirCreateService = ForgeDirCreateService;
    type CommandExecutorService = ForgeCommandExecutorService;
    type McpServer = ForgeMcpServer;

    fn environment_service(&self) -> &Self::EnvironmentService {
        &self.environment_service
    }

    fn file_read_service(&self) -> &Self::FsReadService {
        &self.file_read_service
    }

    fn file_write_service(&self) -> &Self::FsWriteService {
        &self.file_write_service
    }

    fn file_meta_service(&self) -> &Self::FsMetaService {
        &self.file_meta_service
    }

    fn file_snapshot_service(&self) -> &Self::FsSnapshotService {
        &self.file_snapshot_service
    }

    fn file_remove_service(&self) -> &Self::FsRemoveService {
        &self.file_remove_service
    }

    fn file_undo_service(&self) -> &Self::FsUndoService {
        &self.file_undo_service
    }

    fn dir_create_service(&self) -> &Self::DirCreateService {
        &self.dir_create_service
    }

    fn command_executor_service(&self) -> &Self::CommandExecutorService {
        &self.command_executor_service
    }

    fn mcp_server(&self) -> &Self::McpServer {
        &self.mcp_server
    }
}
