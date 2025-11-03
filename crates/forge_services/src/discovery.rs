use std::sync::Arc;

use anyhow::Result;
use forge_app::domain::File;
use forge_app::{EnvironmentInfra, FileDiscoveryService, Walker, WalkerInfra};

pub struct ForgeDiscoveryService<F> {
    service: Arc<F>,
}

impl<F> ForgeDiscoveryService<F> {
    pub fn new(service: Arc<F>) -> Self {
        Self { service }
    }
}

impl<F: EnvironmentInfra + WalkerInfra> ForgeDiscoveryService<F> {
    async fn discover_with_config(&self, config: Walker) -> Result<Vec<File>> {
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
    async fn collect_files(&self, config: Walker) -> Result<Vec<File>> {
        self.discover_with_config(config).await
    }
}
