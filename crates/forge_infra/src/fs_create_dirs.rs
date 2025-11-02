use std::path::Path;

use forge_app::FileDirectoryInfra;

#[derive(Default)]
pub struct ForgeCreateDirsService;

#[async_trait::async_trait]
impl FileDirectoryInfra for ForgeCreateDirsService {
    async fn create_dirs(&self, path: &Path) -> anyhow::Result<()> {
        Ok(forge_fs::ForgeFS::create_dir_all(path).await?)
    }
}
