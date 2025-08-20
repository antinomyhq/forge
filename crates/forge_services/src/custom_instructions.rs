use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use forge_app::domain::Environment;
use forge_app::CustomInstructionsService;

use crate::infra::{EnvironmentInfra, FileReaderInfra};
use crate::utils::get_git_root;

/// This service looks for AGENTS.md files in three locations in order of
/// priority:
/// 1. Base path (environment.base_path)
/// 2. Git root directory (if available)
/// 3. Current working directory (environment.cwd)
#[derive(Clone)]
pub struct ForgeCustomInstructionsService<F> {
    infra: Arc<F>,
}

impl<F: EnvironmentInfra + FileReaderInfra> ForgeCustomInstructionsService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    pub async fn get_custom_instructions_impl(&self) -> Result<Vec<String>> {
        let environment = self.infra.get_environment();
        let paths = self.discover_agents_files(&environment).await;

        let mut custom_instructions = Vec::new();

        for path in paths {
            if let Ok(content) = self.infra.read_utf8(&path).await {
                custom_instructions.push(content);
            }
        }

        Ok(custom_instructions)
    }

    async fn discover_agents_files(&self, environment: &Environment) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        let base_agent_md = environment.base_path.join("AGENTS.md");
        if !paths.contains(&base_agent_md) {
            paths.push(base_agent_md);
        }

        if let Some(git_root_path) = get_git_root(&environment.cwd).await {
            let git_agent_md = git_root_path.join("AGENTS.md");
            if !paths.contains(&git_agent_md) {
                paths.push(git_agent_md);
            }
        }

        let cwd_agent_md = environment.cwd.join("AGENTS.md");
        if !paths.contains(&cwd_agent_md) {
            paths.push(cwd_agent_md);
        }

        paths
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + FileReaderInfra> CustomInstructionsService for ForgeCustomInstructionsService<F> {
    async fn get_custom_instructions(&self) -> Vec<String> {
        self.get_custom_instructions_impl().await.unwrap_or_default()
    }
}
