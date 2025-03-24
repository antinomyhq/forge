use std::sync::Arc;

use forge_domain::Tool;

use super::fetch::Fetch;
use super::fs::*;
use super::patch::*;
use super::shell::Shell;
use super::show_user::ShowUser;
use crate::{EnvironmentService, Infrastructure};

pub struct ToolRegistry<F> {
    infra: Arc<F>,
}

impl<F: Infrastructure> ToolRegistry<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    /// Returns all available tools configured with the given infrastructure
    pub fn tools(&self) -> Vec<Tool> {
        let env = self.infra.environment_service().get_environment();
        vec![
            FSRead::new(self.infra.clone()).into(),
            FSWrite::new(self.infra.clone()).into(),
            FSRemove::new(self.infra.clone()).into(),
            FSList::default().into(),
            FSSearch.into(),
            FSFileInfo.into(),
            ApplyPatchJson::new(self.infra.clone()).into(),
            Shell::new(env.clone()).into(),
            Fetch::default().into(),
            ShowUser.into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use bytes::Bytes;
    use forge_domain::{Environment, Point, Provider, Query, Suggestion};
    use forge_snaps::Snapshot;

    use super::*;
    use crate::{
        EmbeddingService, FileRemoveService, FsCreateDirsService, FsMetaService, FsReadService,
        FsSnapshotService, FsWriteService, VectorIndex,
    };

    /// Create a default test environment
    fn stub() -> Stub {
        Stub {
            env: Environment {
                os: std::env::consts::OS.to_string(),
                cwd: std::env::current_dir().unwrap_or_default(),
                home: Some("/".into()),
                shell: if cfg!(windows) {
                    "cmd.exe".to_string()
                } else {
                    "/bin/sh".to_string()
                },
                base_path: PathBuf::new(),
                qdrant_key: Default::default(),
                qdrant_cluster: Default::default(),
                pid: std::process::id(),
                openai_key: Default::default(),
                provider: Provider::anthropic("test-key"),
            },
        }
    }

    #[derive(Clone)]
    struct Stub {
        env: Environment,
    }

    #[async_trait::async_trait]
    impl EmbeddingService for Stub {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl EnvironmentService for Stub {
        fn get_environment(&self) -> Environment {
            self.env.clone()
        }
    }
    #[async_trait::async_trait]
    impl FsReadService for Stub {
        async fn read(&self, _path: &Path) -> anyhow::Result<Bytes> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FsWriteService for Stub {
        async fn write(&self, _: &Path, _: Bytes) -> anyhow::Result<()> {
            unimplemented!()
        }
    }
    #[async_trait::async_trait]
    impl VectorIndex<Suggestion> for Stub {
        async fn store(&self, _information: Point<Suggestion>) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn search(&self, _query: Query) -> anyhow::Result<Vec<Point<Suggestion>>> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FsSnapshotService for Stub {
        async fn create_snapshot(&self, _: &Path) -> anyhow::Result<Snapshot> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FsMetaService for Stub {
        async fn is_file(&self, _: &Path) -> anyhow::Result<bool> {
            unimplemented!()
        }

        async fn exists(&self, _: &Path) -> anyhow::Result<bool> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FileRemoveService for Stub {
        async fn remove(&self, _: &Path) -> anyhow::Result<()> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FsCreateDirsService for Stub {
        async fn create_dirs(&self, _: &Path) -> anyhow::Result<()> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl Infrastructure for Stub {
        type EnvironmentService = Stub;
        type FsReadService = Stub;
        type FsWriteService = Stub;
        type FsRemoveService = Stub;
        type VectorIndex = Stub;
        type EmbeddingService = Stub;
        type FsMetaService = Stub;
        type FsSnapshotService = Stub;
        type FsCreateDirsService = Stub;

        fn environment_service(&self) -> &Self::EnvironmentService {
            self
        }

        fn file_read_service(&self) -> &Self::FsReadService {
            self
        }

        fn file_write_service(&self) -> &Self::FsWriteService {
            self
        }

        fn vector_index(&self) -> &Self::VectorIndex {
            self
        }

        fn embedding_service(&self) -> &Self::EmbeddingService {
            self
        }

        fn file_meta_service(&self) -> &Self::FsMetaService {
            self
        }

        fn file_snapshot_service(&self) -> &Self::FsSnapshotService {
            self
        }
        fn file_remove_service(&self) -> &Self::FsRemoveService {
            self
        }

        fn create_dirs_service(&self) -> &Self::FsCreateDirsService {
            self
        }
    }

    #[test]
    fn test_tool_description_length() {
        const MAX_DESCRIPTION_LENGTH: usize = 1024;

        println!("\nTool description lengths:");

        let mut any_exceeded = false;
        let stub = Arc::new(stub());
        let registry = ToolRegistry::new(stub.clone());
        for tool in registry.tools() {
            let desc_len = tool.definition.description.len();
            println!(
                "{:?}: {} chars {}",
                tool.definition.name,
                desc_len,
                if desc_len > MAX_DESCRIPTION_LENGTH {
                    "(!)"
                } else {
                    ""
                }
            );

            if desc_len > MAX_DESCRIPTION_LENGTH {
                any_exceeded = true;
            }
        }

        assert!(
            !any_exceeded,
            "One or more tools exceed the maximum description length of {}",
            MAX_DESCRIPTION_LENGTH
        );
    }
}
