use std::path::Path;

use async_jsonl::{JsonlDeserialize, JsonlReader};
use forge_domain::Buffer;
use forge_services::BufferService;
use futures::StreamExt;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;

pub struct ForgeBufferService {}

impl Default for ForgeBufferService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeBufferService {
    pub fn new() -> Self {
        Self {}
    }
    async fn file(&self, path: &Path) -> anyhow::Result<File> {
        Ok(OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .await?)
    }
    async fn write(&self, path: &Path, buffer: Buffer) -> anyhow::Result<()> {
        let mut string = serde_json::to_string(&buffer)?;
        string.push('\n');
        self.file(path).await?.write_all(string.as_bytes()).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl BufferService for ForgeBufferService {
    async fn read_last(
        &self,
        path: &Path,
        n: usize,
    ) -> anyhow::Result<Vec<anyhow::Result<Buffer>>> {
        let file = File::open(path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open file: {}", e))?;
        let iter = async_jsonl::Jsonl::new(file)
            .last_n(n)
            .await?
            .deserialize::<Buffer>()
            .collect::<Vec<_>>()
            .await;

        Ok(iter)
    }

    async fn write(&self, path: &Path, buffer: Buffer) -> anyhow::Result<()> {
        self.write(path, buffer).await
    }
}
