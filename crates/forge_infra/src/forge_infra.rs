use std::sync::Arc;

use forge_services::{EnvironmentService, Infrastructure};

use crate::buffer_service::ForgeBufferService;
use crate::console::ForgeConsoleService;
use crate::env::ForgeEnvironmentService;
use crate::executor::ForgeCommandExecutorService;
use crate::fs_create_dirs::ForgeCreateDirsService;
use crate::fs_meta::ForgeFileMetaService;
use crate::fs_read::ForgeFileReadService;
use crate::fs_remove::ForgeFileRemoveService;
use crate::fs_snap::ForgeFileSnapshotService;
use crate::fs_write::ForgeFileWriteService;
use crate::inquire::ForgeInquire;
use crate::mcp_server::ForgeMcpServer;

#[derive(Clone)]
pub struct ForgeInfra {
    file_read_service: Arc<ForgeFileReadService>,
    file_write_service: Arc<ForgeFileWriteService<ForgeFileSnapshotService>>,
    environment_service: Arc<ForgeEnvironmentService>,
    file_snapshot_service: Arc<ForgeFileSnapshotService>,
    file_meta_service: Arc<ForgeFileMetaService>,
    file_remove_service: Arc<ForgeFileRemoveService<ForgeFileSnapshotService>>,
    create_dirs_service: Arc<ForgeCreateDirsService>,
    command_executor_service: Arc<ForgeCommandExecutorService>,
    inquire_service: Arc<ForgeInquire>,
    mcp_server: ForgeMcpServer,
    forge_buffer_service: Arc<ForgeBufferService>,
    console_service: Arc<ForgeConsoleService>,
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
            environment_service,
            file_snapshot_service,
            create_dirs_service: Arc::new(ForgeCreateDirsService),
            command_executor_service: Arc::new(ForgeCommandExecutorService::new(
                restricted,
                env.clone(),
            )),
            inquire_service: Arc::new(ForgeInquire::new()),
            mcp_server: ForgeMcpServer,
            forge_buffer_service: Arc::new(ForgeBufferService::new()),
            console_service: Arc::new(ForgeConsoleService),
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
    type FsCreateDirsService = ForgeCreateDirsService;
    type CommandExecutorService = ForgeCommandExecutorService;
    type InquireService = ForgeInquire;
    type McpServer = ForgeMcpServer;
    type BufferService = ForgeBufferService;
    type ConsolePrintService = ForgeConsoleService;

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

    fn create_dirs_service(&self) -> &Self::FsCreateDirsService {
        &self.create_dirs_service
    }

    fn command_executor_service(&self) -> &Self::CommandExecutorService {
        &self.command_executor_service
    }

    fn inquire_service(&self) -> &Self::InquireService {
        &self.inquire_service
    }

    fn mcp_server(&self) -> &Self::McpServer {
        &self.mcp_server
    }

    fn buffer_service(&self) -> &Self::BufferService {
        &self.forge_buffer_service
    }
    fn console_print_service(&self) -> &Self::ConsolePrintService {
        self.console_service.as_ref()
    }
}
