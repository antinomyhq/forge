use bytes::Bytes;
use forge_services::HttpInfra;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::{Client, Response};
use tracing::debug;

const VERSION: &str = match option_env!("APP_VERSION") {
    None => env!("CARGO_PKG_VERSION"),
    Some(v) => v,
};

#[derive(Default)]
pub struct ForgeHttpService {
    client: Client,
}

impl ForgeHttpService {
    pub fn new() -> Self {
        Default::default()
    }
    async fn get(&self, url: &str, headers: Option<HeaderMap>) -> anyhow::Result<Response> {
        Ok(self
            .client
            .get(url)
            .header("User-Agent", "Forge")
            .headers(self.headers(headers))
            .send()
            .await?)
    }
    async fn post(&self, url: &str, body: Bytes) -> anyhow::Result<Response> {
        Ok(self
            .client
            .post(url)
            .headers(self.headers(None))
            .body(body)
            .send()
            .await?)
    }
    async fn delete(&self, url: &str) -> anyhow::Result<Response> {
        Ok(self
            .client
            .delete(url)
            .headers(self.headers(None))
            .send()
            .await?)
    }

    // OpenRouter optional headers ref: https://openrouter.ai/docs/api-reference/overview#headers
    // - `HTTP-Referer`: Identifies your app on openrouter.ai
    // - `X-Title`: Sets/modifies your app's title
    fn headers(&self, headers: Option<HeaderMap>) -> HeaderMap {
        let mut headers = headers.unwrap_or_default();
        headers.insert("X-Title", HeaderValue::from_static("forge"));
        headers.insert(
            "x-app-version",
            HeaderValue::from_str(format!("v{VERSION}").as_str())
                .unwrap_or(HeaderValue::from_static("v0.1.0-dev")),
        );
        headers.insert(
            "HTTP-Referer",
            HeaderValue::from_static("https://forgecode.dev"),
        );
        headers.insert(
            reqwest::header::CONNECTION,
            HeaderValue::from_static("keep-alive"),
        );
        debug!(headers = ?Self::sanitize_headers(&headers), "Request Headers");
        headers
    }

    fn sanitize_headers(headers: &HeaderMap) -> HeaderMap {
        let sensitive_headers = [AUTHORIZATION.as_str()];
        headers
            .iter()
            .map(|(name, value)| {
                let name_str = name.as_str().to_lowercase();
                let value_str = if sensitive_headers.contains(&name_str.as_str()) {
                    HeaderValue::from_static("[REDACTED]")
                } else {
                    value.clone()
                };
                (name.clone(), value_str)
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl HttpInfra for ForgeHttpService {
    async fn get(&self, url: &str, headers: Option<HeaderMap>) -> anyhow::Result<Response> {
        self.get(url, headers).await
    }

    async fn post(&self, url: &str, body: Bytes) -> anyhow::Result<Response> {
        self.post(url, body).await
    }

    async fn delete(&self, url: &str) -> anyhow::Result<Response> {
        self.delete(url).await
    }
}
