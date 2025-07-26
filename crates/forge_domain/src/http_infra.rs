use std::pin::Pin;

use anyhow::Context;
use bytes::Bytes;
use reqwest::header::HeaderMap;
use reqwest::{Response, Url};
use tokio_stream::Stream;
/// HTTP infrastructure trait for making HTTP requests
#[async_trait::async_trait]
pub trait HttpInfra: Send + Sync + 'static {
    async fn get(&self, url: &Url, headers: Option<HeaderMap>) -> anyhow::Result<Response>;
    async fn post(&self, url: &Url, body: Bytes) -> anyhow::Result<Response>;
    async fn delete(&self, url: &Url) -> anyhow::Result<Response>;

    /// Posts JSON data and returns a server-sent events stream
    async fn post_stream(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<ServerSentEvent>> + Send>>>;

    fn url(&self, base_url: &str, path: &str) -> anyhow::Result<Url> {
        // Validate the path doesn't contain certain patterns
        if path.contains("://") || path.contains("..") {
            anyhow::bail!("Invalid path: Contains forbidden patterns");
        }

        // Remove leading slash to avoid double slashes
        let path = path.trim_start_matches('/');

        let url = Url::parse(base_url)
            .with_context(|| format!("Failed to parse base URL: {base_url}"))?
            .join(path)
            .with_context(|| format!("Failed to append {path} to base URL: {base_url}"))?;
        Ok(url)
    }



    fn resolve_headers(&self, headers: Vec<(String, String)>) -> HeaderMap {
        let mut header_map = HeaderMap::new();
        for (key, value) in headers {
            let header_name = reqwest::header::HeaderName::from_bytes(key.as_bytes())
                .expect("Invalid header name");
            let header_value = value.parse().expect("Invalid header value");
            header_map.insert(header_name, header_value);
        }
        header_map
    }

    fn format_http_context(&self, method: Option<&str>, url: &str) -> String {
        let method = method.unwrap_or("GET");
        format!("{method} request to {url}")
    }
}

/// Represents a server-sent event
#[derive(Debug, Clone)]
pub struct ServerSentEvent {
    pub event_type: Option<String>,
    pub data: String,
    pub id: Option<String>,
}

/// Event stream states
#[derive(Debug)]
pub enum EventStreamState {
    Open,
    Message(ServerSentEvent),
    Done,
    Error(anyhow::Error),
}
