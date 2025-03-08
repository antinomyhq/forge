mod fetch;
mod fs;
mod patch;
mod shell;
mod syn;
mod think;
mod utils;

use std::sync::Arc;

use fetch::Fetch;
use forge_domain::Tool;
use fs::*;
use patch::*;
use shell::Shell;
use think::Think;

use crate::{EnvironmentService, Infrastructure};

pub fn tools<F: Infrastructure>(infra: Arc<F>) -> Vec<Tool> {
    let env = infra.environment_service().get_environment();
    vec![
        FSRead.into(),
        FSWrite::new(infra.clone()).into(),
        // @ssddOnTop need to keep a back up on file removal
        FSRemove.into(),
        FSList::default().into(),
        FSSearch.into(),
        FSFileInfo.into(),
        // ApplyPatch::new(infra.clone()).into(),
        ApplyPatchJson::new(infra).into(),
        Shell::new(env.clone()).into(),
        Think::default().into(),
        Fetch::default().into(),
    ]
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use bytes::Bytes;
    use forge_domain::{Environment, Point, Provider, Query, Suggestion};
    use forge_snaps::{SnapshotInfo, SnapshotMetadata};

    use super::*;
    use crate::{
        EmbeddingService, FileMetaService, FileReadService, FileSnapshotService, FileWriteService,
        VectorIndex,
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
    impl FileReadService for Stub {
        async fn read(&self, _path: &Path) -> anyhow::Result<Bytes> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FileWriteService for Stub {
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
    impl FileSnapshotService for Stub {
        fn snapshot_dir(&self) -> PathBuf {
            unimplemented!()
        }

        async fn create_snapshot(&self, _: &Path) -> anyhow::Result<SnapshotInfo> {
            unimplemented!()
        }

        async fn list_snapshots(&self, _: &Path) -> anyhow::Result<Vec<SnapshotInfo>> {
            unimplemented!()
        }

        async fn restore_by_timestamp(&self, _: &Path, _: &str) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn restore_by_index(&self, _: &Path, _: isize) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn restore_previous(&self, _: &Path) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn get_snapshot_by_timestamp(
            &self,
            _: &Path,
            _: &str,
        ) -> anyhow::Result<SnapshotMetadata> {
            unimplemented!()
        }

        async fn get_snapshot_by_index(
            &self,
            _: &Path,
            _: isize,
        ) -> anyhow::Result<SnapshotMetadata> {
            unimplemented!()
        }

        async fn purge_older_than(&self, _: u32) -> anyhow::Result<usize> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FileMetaService for Stub {
        async fn is_file(&self, _: &Path) -> anyhow::Result<bool> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl Infrastructure for Stub {
        type EnvironmentService = Stub;
        type FileReadService = Stub;
        type FileWriteService = Stub;
        type VectorIndex = Stub;
        type EmbeddingService = Stub;
        type FileMetaService = Stub;
        type FileSnapshotService = Stub;

        fn environment_service(&self) -> &Self::EnvironmentService {
            self
        }

        fn file_read_service(&self) -> &Self::FileReadService {
            self
        }

        fn file_write_service(&self) -> &Self::FileWriteService {
            self
        }

        fn vector_index(&self) -> &Self::VectorIndex {
            self
        }

        fn embedding_service(&self) -> &Self::EmbeddingService {
            self
        }

        fn file_meta_service(&self) -> &Self::FileMetaService {
            self
        }

        fn file_snapshot_service(&self) -> &Self::FileSnapshotService {
            self
        }
    }

    #[test]
    fn test_tool_description_length() {
        const MAX_DESCRIPTION_LENGTH: usize = 1024;

        println!("\nTool description lengths:");

        let mut any_exceeded = false;
        let stub = Arc::new(stub());
        for tool in tools(stub.clone()) {
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
