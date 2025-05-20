use bytes::Bytes;
use forge_services::HttpService;
use reqwest::{Client, Response};

#[derive(Default)]
pub struct ForgeHttpService {
    client: Client,
}

impl ForgeHttpService {
    pub fn new() -> Self {
        Default::default()
    }
    async fn body(resp: Response) -> anyhow::Result<Bytes> {
        if resp.status().is_success() {
            Ok(resp.bytes().await?)
        } else {
            Err(anyhow::anyhow!("Failed to fetch URL: {}", resp.url()))
        }
    }
    async fn get(&self, url: &str) -> anyhow::Result<Bytes> {
        let response = self
            .client
            .get(url)
            .header("User-Agent", "Forge")
            .send()
            .await?;
        Self::body(response).await
    }
    async fn post(&self, url: &str, body: Bytes) -> anyhow::Result<Bytes> {
        let response = self
            .client
            .post(url)
            .header("User-Agent", "Forge")
            .body(body)
            .send()
            .await?;
        Self::body(response).await
    }
}

#[async_trait::async_trait]
impl HttpService for ForgeHttpService {
    async fn get(&self, url: &str) -> anyhow::Result<Bytes> {
        self.get(url).await
    }

    async fn post(&self, url: &str, body: Bytes) -> anyhow::Result<Bytes> {
        self.post(url, body).await
    }
}
