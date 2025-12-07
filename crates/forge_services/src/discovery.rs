use std::sync::Arc;

use anyhow::Result;
use forge_app::domain::File;
use forge_app::{
    DirectoryReaderInfra, EnvironmentInfra, FileDiscoveryService, Walker, WalkerInfra,
};

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
impl<F: EnvironmentInfra + WalkerInfra + DirectoryReaderInfra + Send + Sync> FileDiscoveryService
    for ForgeDiscoveryService<F>
{
    async fn collect_files(&self, config: Walker) -> Result<Vec<File>> {
        self.discover_with_config(config).await
    }

    async fn list_current_directory(&self) -> Result<Vec<File>> {
        let env = self.service.get_environment();
        let entries = self.service.list_directory_entries(&env.cwd).await?;

        let mut files: Vec<File> = entries
            .into_iter()
            .map(|(path, is_dir)| File {
                path: path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string(),
                is_dir,
            })
            .collect();

        // Sort: directories first (alphabetically), then files (alphabetically)
        files.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.path.cmp(&b.path),
        });

        Ok(files)
    }
}
