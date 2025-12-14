use std::sync::Arc;

use bytes::Bytes;
use forge_app::{HttpClientService, HttpInfra};
use reqwest::Response;
use reqwest::header::HeaderMap;
use url::Url;

#[derive(Clone)]
pub struct HttpClient<I>(Arc<I>);

impl<I: HttpInfra> HttpClient<I> {
    pub fn new(infra: Arc<I>) -> Self {
        HttpClient(infra)
    }
}

#[async_trait::async_trait]
impl<T: HttpInfra> HttpClientService for HttpClient<T> {
    async fn get(&self, url: &Url, headers: Option<HeaderMap>) -> anyhow::Result<Response> {
        self.0.http_get(url, headers).await
    }
    async fn post(&self, url: &Url, body: bytes::Bytes) -> anyhow::Result<Response> {
        self.0.http_post(url, body).await
    }
    async fn delete(&self, url: &Url) -> anyhow::Result<Response> {
        self.0.http_delete(url).await
    }

    /// Posts JSON data and returns a server-sent events stream
    async fn eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<reqwest_eventsource::EventSource> {
        self.0.http_eventsource(url, headers, body).await
    }
}
