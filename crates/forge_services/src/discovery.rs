use std::sync::Arc;

use anyhow::Result;
use forge_app::{EnvironmentService, FileDiscoveryService};
use forge_domain::File;
use forge_walker::Walker;

use crate::Infrastructure;

pub struct ForgeDiscoveryService<F> {
    domain: Arc<F>,
}

impl<F> ForgeDiscoveryService<F> {
    pub fn new(domain: Arc<F>) -> Self {
        Self { domain }
    }
}

impl<F: Infrastructure> ForgeDiscoveryService<F> {
    async fn discover_with_depth(&self, max_depth: Option<usize>) -> Result<Vec<File>> {
        let cwd = self
            .domain
            .environment_service()
            .get_environment()
            .cwd
            .clone();

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
impl<F: Infrastructure + Send + Sync> FileDiscoveryService for ForgeDiscoveryService<F> {
    async fn collect(&self, max_depth: Option<usize>) -> Result<Vec<File>> {
        self.discover_with_depth(max_depth).await
    }
}
