use std::sync::Arc;

use anyhow::Result;
use forge_app::FileDiscoveryService;
use forge_domain::File;

use crate::{EnvironmentInfra, WalkerConfig, WalkerInfra};

pub struct ForgeDiscoveryService<F> {
    service: Arc<F>,
}

impl<F> ForgeDiscoveryService<F> {
    pub fn new(service: Arc<F>) -> Self {
        Self { service }
    }
}

impl<F: EnvironmentInfra + WalkerInfra> ForgeDiscoveryService<F> {
    async fn discover_with_depth(&self, max_depth: Option<usize>) -> Result<Vec<File>> {
        let cwd = self.service.get_environment().cwd.clone();

        let config = if let Some(depth) = max_depth {
            WalkerConfig::unlimited().cwd(cwd).max_depth(depth)
        } else {
            WalkerConfig::unlimited().cwd(cwd)
        };

        let files = self.service.walk(config).await?;
        Ok(files
            .into_iter()
            .map(|file| File { path: file.path.clone(), is_dir: file.is_dir() })
            .collect())
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + WalkerInfra + Send + Sync> FileDiscoveryService
    for ForgeDiscoveryService<F>
{
    async fn collect(&self, max_depth: Option<usize>) -> Result<Vec<File>> {
        self.discover_with_depth(max_depth).await
    }
}
