use std::sync::Arc;

use anyhow::Result;
use forge_app::FileDiscoveryService;
use forge_domain::File;
use forge_walker::Walker;

use crate::EnvironmentInfra;

pub struct ForgeDiscoveryService<F> {
    env_service: Arc<F>,
}

impl<F> ForgeDiscoveryService<F> {
    pub fn new(env_service: Arc<F>) -> Self {
        Self { env_service }
    }
}

impl<F: EnvironmentInfra> ForgeDiscoveryService<F> {
    async fn discover_with_depth(&self, max_depth: Option<usize>) -> Result<Vec<File>> {
        let cwd = self.env_service.get_environment().cwd.clone();

        let mut walker = Walker::max_all().cwd(cwd);
        if let Some(depth) = max_depth {
            walker = walker.max_depth(depth);
        }

        let files = walker.get().await?;
        Ok(files
            .into_iter()
            .map(|file| File { path: file.path.clone(), is_dir: file.is_dir() })
            .collect())
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + Send + Sync> FileDiscoveryService for ForgeDiscoveryService<F> {
    async fn collect(&self, max_depth: Option<usize>) -> Result<Vec<File>> {
        self.discover_with_depth(max_depth).await
    }
}
