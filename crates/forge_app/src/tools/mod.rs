mod fetch;
mod fs;
mod knowledge;
mod patch;
mod shell;
mod syn;
mod think;
mod utils;

use std::sync::Arc;

use fetch::Fetch;
use forge_domain::{SuggestionService, Tool};
use fs::*;
use knowledge::{RecallSuggestions, StoreSuggestion};
use patch::*;
use shell::Shell;
use think::Think;

use crate::{EnvironmentService, Infrastructure};

pub fn tools<F: Infrastructure, S: SuggestionService>(infra: Arc<F>, suggest: Arc<S>) -> Vec<Tool> {
    let env = infra.environment_service().get_environment();
    vec![
        FSRead::new(infra.clone()).into(),
        FSWrite::new(infra.clone()).into(),
        FSRemove.into(),
        FSList::default().into(),
        FSSearch::new(infra.clone()).into(),
        FSFileInfo.into(),
        // TODO: once ApplyPatchJson is stable we can delete ApplyPatch
        ApplyPatch::new(infra.clone()).into(),
        // ApplyPatchJson.into(),
        Shell::new(env.clone()).into(),
        Think::default().into(),
        Fetch::default().into(),
        RecallSuggestions::new(suggest.clone()).into(),
        StoreSuggestion::new(suggest.clone()).into(),
    ]
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::{Path, PathBuf};

    use forge_domain::{Environment, Point, Query, Suggestion};

    use super::*;
    use crate::{EmbeddingService, FileReadService, FileWriteService, VectorIndex};

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

    pub struct Stub {
        env: Environment,
    }

    impl Default for Stub {
        fn default() -> Self {
            stub()
        }
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
        async fn read(&self, path: &Path) -> anyhow::Result<String> {
            let content = tokio::fs::read_to_string(path).await?;
            Ok(content)
        }
    }

    #[async_trait::async_trait]
    impl FileWriteService for Stub {
        async fn write(&self, path: &Path, content: &str) -> anyhow::Result<()> {
            tokio::fs::write(path, content).await?;
            Ok(())
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
        type FileWriteService = Stub;
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

        fn file_write_service(&self) -> &Self::FileReadService {
            self
        }
    }

    #[async_trait::async_trait]
    impl SuggestionService for Stub {
        async fn search(&self, _request: &str) -> anyhow::Result<Vec<Suggestion>> {
            unimplemented!()
        }
        async fn insert(&self, _suggestion: Suggestion) -> anyhow::Result<()> {
            unimplemented!()
        }
    }

    #[test]
    fn test_tool_description_length() {
        const MAX_DESCRIPTION_LENGTH: usize = 1024;

        println!("\nTool description lengths:");

        let mut any_exceeded = false;
        let stub = Arc::new(stub());
        for tool in tools(stub.clone(), stub.clone()) {
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
