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
        FSWrite.into(),
        FSRemove.into(),
        FSList::default().into(),
        FSSearch.into(),
        FSFileInfo.into(),
        // TODO: once ApplyPatchJson is stable we can delete ApplyPatch
        // ApplyPatch.into(),
        ApplyPatchJson.into(),
        Shell::new(env.clone()).into(),
        Think::default().into(),
        Fetch::default().into(),
    ]
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use forge_domain::{Environment, Point, Query, Suggestion};

    use super::*;
    use crate::{EmbeddingService, FileReadService, VectorIndex};

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
                provider_url: Default::default(),
                provider_key: Default::default(),
                openai_key: Default::default(),
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
        async fn read(&self, _path: &Path) -> anyhow::Result<String> {
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
    impl Infrastructure for Stub {
        type EnvironmentService = Stub;
        type FileReadService = Stub;
        type VectorIndex = Stub;
        type EmbeddingService = Stub;

        fn environment_service(&self) -> &Self::EnvironmentService {
            self
        }

        fn file_read_service(&self) -> &Self::FileReadService {
            self
        }

        fn vector_index(&self) -> &Self::VectorIndex {
            self
        }

        fn embedding_service(&self) -> &Self::EmbeddingService {
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
