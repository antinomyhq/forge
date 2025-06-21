use bytes::Bytes;
use forge_services::HttpInfra;
use reqwest::{Client, Response};

#[derive(Default)]
pub struct ForgeHttpService {
    client: Client,
}

impl ForgeHttpService {
    pub fn new() -> Self {
        Default::default()
    }
    async fn get(&self, url: &str) -> anyhow::Result<Response> {
        Ok(self
            .client
            .get(url)
            .header("User-Agent", "Forge")
            .send()
            .await?)
    }
    async fn post(&self, url: &str, body: Bytes) -> anyhow::Result<Response> {
        Ok(self
            .client
            .post(url)
            .header("User-Agent", "Forge")
            .body(body)
            .send()
            .await?)
    }
    async fn delete(&self, url: &str) -> anyhow::Result<Response> {
        Ok(self
            .client
            .delete(url)
            .header("User-Agent", "Forge")
            .send()
            .await?)
    }
}

#[async_trait::async_trait]
impl HttpInfra for ForgeHttpService {
    async fn get(&self, url: &str) -> anyhow::Result<Response> {
        self.get(url).await
    }

    async fn post(&self, url: &str, body: Bytes) -> anyhow::Result<Response> {
        self.post(url, body).await
    }

    async fn delete(&self, url: &str) -> anyhow::Result<Response> {
        self.delete(url).await
    }
}
