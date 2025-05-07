use std::sync::Arc;

use forge_domain::Tool;

use super::completion::Completion;
use super::fetch::Fetch;
use super::fs::*;
use super::patch::*;
use super::shell::Shell;
use crate::tools::followup::Followup;
use crate::Infrastructure;

pub struct ToolRegistry<F> {
    infra: Arc<F>,
}

impl<F: Infrastructure> ToolRegistry<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    /// Returns all available tools configured with the given infrastructure
    pub fn tools(&self) -> Vec<Tool> {
        vec![
            FSRead::new(self.infra.clone()).into(),
            FSWrite::new(self.infra.clone()).into(),
            FSRemove::new(self.infra.clone()).into(),
            FSList::default().into(),
            FSFind::new(self.infra.clone()).into(),
            FSFileInfo::new(self.infra.clone()).into(),
            FsUndo::new(self.infra.clone()).into(),
            ApplyPatchJson::new(self.infra.clone()).into(),
            Shell::new(self.infra.clone()).into(),
            Completion.into(),
            Followup::new(self.infra.clone()).into(),
            Fetch::new(self.infra.clone()).into(),
        ]
    }
}

#[cfg(test)]
pub mod tests {
    use std::path::{Path, PathBuf};

    use bytes::Bytes;
    use forge_domain::{CommandOutput, Environment, EnvironmentService, Provider};
    use forge_snaps::Snapshot;

    use super::*;
    use crate::{
        CommandExecutorService, FileRemoveService, FsCreateDirsService, FsMetaService,
        FsReadService, FsSnapshotService, FsWriteService, InquireService,
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
                pid: std::process::id(),
                provider: Provider::anthropic("test-key"),
                retry_config: Default::default(),
            },
        }
    }

    impl Default for Stub {
        fn default() -> Self {
            stub()
        }
    }

    #[derive(Clone)]
    pub struct Stub {
        env: Environment,
    }

    #[async_trait::async_trait]
    impl EnvironmentService for Stub {
        fn get_environment(&self) -> Environment {
            self.env.clone()
        }
    }

    #[async_trait::async_trait]
    impl FsReadService for Stub {
        async fn read_utf8(&self, _path: &Path) -> anyhow::Result<String> {
            unimplemented!()
        }

        async fn read(&self, _path: &Path) -> anyhow::Result<Vec<u8>> {
            unimplemented!()
        }

        async fn range_read_utf8(
            &self,
            _path: &Path,
            _start_char: u64,
            _end_char: u64,
        ) -> anyhow::Result<(String, forge_fs::FileInfo)> {
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
    impl FsSnapshotService for Stub {
        async fn create_snapshot(&self, _: &Path) -> anyhow::Result<Snapshot> {
            unimplemented!()
        }

        async fn undo_snapshot(&self, _: &Path) -> anyhow::Result<()> {
            Ok(())
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
    impl CommandExecutorService for Stub {
        async fn execute_command(&self, _: String, _: PathBuf) -> anyhow::Result<CommandOutput> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl InquireService for Stub {
        /// Prompts the user with question
        async fn prompt_question(&self, question: &str) -> anyhow::Result<Option<String>> {
            // For testing, we can just return the question as the answer
            Ok(Some(question.to_string()))
        }

        /// Prompts the user to select a single option from a list
        async fn select_one(
            &self,
            _: &str,
            options: Vec<String>,
        ) -> anyhow::Result<Option<String>> {
            // For testing, we can just return the first option
            if options.is_empty() {
                return Err(anyhow::anyhow!("No options provided"));
            }
            Ok(Some(options[0].clone()))
        }

        /// Prompts the user to select multiple options from a list
        async fn select_many(
            &self,
            _: &str,
            options: Vec<String>,
        ) -> anyhow::Result<Option<Vec<String>>> {
            // For testing, we can just return all options
            if options.is_empty() {
                return Err(anyhow::anyhow!("No options provided"));
            }
            Ok(Some(options))
        }
    }

    #[async_trait::async_trait]
    impl Infrastructure for Stub {
        type EnvironmentService = Stub;
        type FsReadService = Stub;
        type FsWriteService = Stub;
        type FsRemoveService = Stub;
        type FsMetaService = Stub;
        type FsSnapshotService = Stub;
        type FsCreateDirsService = Stub;
        type CommandExecutorService = Stub;
        type InquireService = Stub;

        fn environment_service(&self) -> &Self::EnvironmentService {
            self
        }

        fn file_read_service(&self) -> &Self::FsReadService {
            self
        }

        fn file_write_service(&self) -> &Self::FsWriteService {
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

        fn command_executor_service(&self) -> &Self::CommandExecutorService {
            self
        }

        fn inquire_service(&self) -> &Self::InquireService {
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
            "One or more tools exceed the maximum description length of {MAX_DESCRIPTION_LENGTH}"
        );
    }
}
