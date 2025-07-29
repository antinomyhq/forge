use std::pin::Pin;

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
    async fn eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<ServerSentEvent>> + Send>>>;
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
