use std::sync::{Arc, OnceLock};

use anyhow::Context;
use bytes::Bytes;
use forge_app::HttpInfra;
use forge_domain::Environment;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::{Client, Response, StatusCode, Url};
use reqwest_eventsource::{EventSource, RequestBuilderExt};
use tracing::debug;

use crate::http::ClientBuilderExt;

const VERSION: &str = match option_env!("APP_VERSION") {
    None => env!("CARGO_PKG_VERSION"),
    Some(v) => v,
};

pub struct ForgeHttpInfra<F> {
    client: OnceLock<Client>,
    env: Environment,
    file: Arc<F>,
}

impl<F: forge_app::FileWriterInfra + 'static> ForgeHttpInfra<F> {
    pub fn new(env: Environment, file_writer: Arc<F>) -> Self {
        Self { env, client: OnceLock::new(), file: file_writer }
    }

    fn client(&self) -> anyhow::Result<&Client> {
        // Fast path: already initialized.
        if let Some(client) = self.client.get() {
            return Ok(client);
        }

        // Build the client. On failure the error propagates and nothing is
        // stored, so the next call will retry.
        let new_client = reqwest::Client::builder()
            .with_http_config(&self.env.http)
            .with_proxy_fallback()
            .and_then(|b| b.build().map_err(Into::into))?;

        // Store on success. If another thread raced us here and already stored
        // a client, `get_or_init` returns theirs and drops ours — that's fine.
        Ok(self.client.get_or_init(|| new_client))
    }

    async fn get(&self, url: &Url, headers: Option<HeaderMap>) -> anyhow::Result<Response> {
        self.execute_request("GET", url, |client| {
            client.get(url.clone()).headers(self.headers(headers))
        })
        .await
    }

    async fn post(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<Response> {
        let mut request_headers = self.headers(headers);
        request_headers.insert("Content-Type", HeaderValue::from_static("application/json"));

        self.write_debug_request(&body);

        self.execute_request("POST", url, |client| {
            client.post(url.clone()).headers(request_headers).body(body)
        })
        .await
    }

    async fn delete(&self, url: &Url) -> anyhow::Result<Response> {
        self.execute_request("DELETE", url, |client| {
            client.delete(url.clone()).headers(self.headers(None))
        })
        .await
    }

    /// Generic helper method to execute HTTP requests with consistent error
    /// handling
    async fn execute_request<B>(
        &self,
        method: &str,
        url: &Url,
        request_builder: B,
    ) -> anyhow::Result<Response>
    where
        B: FnOnce(&Client) -> reqwest::RequestBuilder,
    {
        let response = request_builder(self.client()?)
            .send()
            .await
            .with_context(|| format_http_context(None, method, url))?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            return Err(anyhow::anyhow!(error_body))
                .with_context(|| format_http_context(Some(status), method, url));
        }

        Ok(response)
    }

    // OpenRouter optional headers ref: https://openrouter.ai/docs/api-reference/overview#headers
    // - `HTTP-Referer`: Identifies your app on openrouter.ai
    // - `X-Title`: Sets/modifies your app's title
    fn headers(&self, headers: Option<HeaderMap>) -> HeaderMap {
        let mut headers = headers.unwrap_or_default();
        // Only set User-Agent if the provider hasn't already set one
        if !headers.contains_key("User-Agent") {
            headers.insert("User-Agent", HeaderValue::from_static("Forge"));
        }
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

    fn write_debug_request(&self, body: &Bytes) {
        if let Some(debug_path) = &self.env.debug_requests {
            let file_writer = self.file.clone();
            let body_clone = body.clone();
            let debug_path = debug_path.clone();
            tokio::spawn(async move {
                let _ = file_writer.write(&debug_path, body_clone).await;
            });
        }
    }

    async fn eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<EventSource> {
        let mut request_headers = self.headers(headers);
        request_headers.insert("Content-Type", HeaderValue::from_static("application/json"));

        self.write_debug_request(&body);

        self.client()?
            .post(url.clone())
            .headers(request_headers)
            .body(body)
            .eventsource()
            .with_context(|| format_http_context(None, "POST (EventSource)", url))
    }
}

fn format_http_context<U: AsRef<str>>(status: Option<StatusCode>, method: &str, url: U) -> String {
    if let Some(status) = status {
        format!("{} {} {}", status.as_u16(), method, url.as_ref())
    } else {
        format!("{} {}", method, url.as_ref())
    }
}

#[async_trait::async_trait]
impl<F: forge_app::FileWriterInfra + 'static> HttpInfra for ForgeHttpInfra<F> {
    async fn http_get(&self, url: &Url, headers: Option<HeaderMap>) -> anyhow::Result<Response> {
        self.get(url, headers).await
    }

    async fn http_post(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<Response> {
        self.post(url, headers, body).await
    }

    async fn http_delete(&self, url: &Url) -> anyhow::Result<Response> {
        self.delete(url).await
    }

    async fn http_eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<EventSource> {
        self.eventsource(url, headers, body).await
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use fake::{Fake, Faker};
    use forge_app::FileWriterInfra;
    use forge_domain::{Environment, HttpConfig};
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Clone)]
    struct MockFileWriter {
        writes: Arc<Mutex<Vec<(PathBuf, Bytes)>>>,
    }

    impl MockFileWriter {
        fn new() -> Self {
            Self { writes: Arc::new(Mutex::new(Vec::new())) }
        }

        async fn get_writes(&self) -> Vec<(PathBuf, Bytes)> {
            self.writes.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl FileWriterInfra for MockFileWriter {
        async fn write(&self, path: &std::path::Path, contents: Bytes) -> anyhow::Result<()> {
            self.writes
                .lock()
                .await
                .push((path.to_path_buf(), contents));
            Ok(())
        }

        async fn write_temp(
            &self,
            _prefix: &str,
            _extension: &str,
            _content: &str,
        ) -> anyhow::Result<PathBuf> {
            Ok(Faker.fake())
        }
    }

    fn create_test_env(debug_requests: Option<PathBuf>) -> Environment {
        Environment { debug_requests, http: HttpConfig::default(), ..Faker.fake() }
    }

    #[tokio::test]
    async fn test_debug_requests_none_does_not_write() {
        let file_writer = MockFileWriter::new();
        let env = create_test_env(None);
        let http = ForgeHttpInfra::new(env, Arc::new(file_writer.clone()));

        let body = Bytes::from("test request body");
        let url = Url::parse("https://api.test.com/messages").unwrap();

        let _ = http.eventsource(&url, None, body).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let writes = file_writer.get_writes().await;
        assert_eq!(
            writes.len(),
            0,
            "No files should be written when debug_requests is None"
        );
    }

    #[tokio::test]
    async fn test_debug_requests_with_valid_path() {
        let file_writer = MockFileWriter::new();
        let debug_path = PathBuf::from("/tmp/forge-test/debug.json");
        let env = create_test_env(Some(debug_path.clone()));
        let http = ForgeHttpInfra::new(env, Arc::new(file_writer.clone()));

        let body = Bytes::from("test request body");
        let url = Url::parse("https://api.test.com/messages").unwrap();

        let _ = http.eventsource(&url, None, body.clone()).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let writes = file_writer.get_writes().await;
        assert_eq!(writes.len(), 1, "Should write one file");
        assert_eq!(writes[0].0, debug_path);
        assert_eq!(writes[0].1, body);
    }

    #[tokio::test]
    async fn test_debug_requests_with_relative_path() {
        let file_writer = MockFileWriter::new();
        let debug_path = PathBuf::from("./debug/requests.json");
        let env = create_test_env(Some(debug_path.clone()));
        let http = ForgeHttpInfra::new(env, Arc::new(file_writer.clone()));

        let body = Bytes::from("test request body");
        let url = Url::parse("https://api.test.com/messages").unwrap();

        let _ = http.eventsource(&url, None, body.clone()).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let writes = file_writer.get_writes().await;
        assert_eq!(writes.len(), 1, "Should write one file");
        assert_eq!(writes[0].0, debug_path);
        assert_eq!(writes[0].1, body);
    }

    #[tokio::test]
    async fn test_debug_requests_post_none_does_not_write() {
        let file_writer = MockFileWriter::new();
        let env = create_test_env(None);
        let http = ForgeHttpInfra::new(env, Arc::new(file_writer.clone()));

        let body = Bytes::from("test request body");
        let url = Url::parse("https://127.0.0.1:9/responses").unwrap();

        let _ = http.post(&url, None, body).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let writes = file_writer.get_writes().await;
        assert_eq!(
            writes.len(),
            0,
            "No files should be written for POST when debug_requests is None"
        );
    }

    #[tokio::test]
    async fn test_debug_requests_post_writes_body() {
        let file_writer = MockFileWriter::new();
        let debug_path = PathBuf::from("/tmp/forge-test/debug-post.json");
        let env = create_test_env(Some(debug_path.clone()));
        let http = ForgeHttpInfra::new(env, Arc::new(file_writer.clone()));

        let body = Bytes::from("test request body");
        let url = Url::parse("https://127.0.0.1:9/responses").unwrap();

        let _ = http.post(&url, None, body.clone()).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let writes = file_writer.get_writes().await;
        assert_eq!(
            writes.len(),
            1,
            "Should write one file for POST when debug_requests is set"
        );
        assert_eq!(writes[0].0, debug_path);
        assert_eq!(writes[0].1, body);
    }

    #[tokio::test]
    async fn test_debug_requests_fallback_on_dir_creation_failure() {
        let file_writer = MockFileWriter::new();
        let debug_path = PathBuf::from("test_debug.json");
        let env = create_test_env(Some(debug_path.clone()));
        let http = ForgeHttpInfra::new(env, Arc::new(file_writer.clone()));

        let body = Bytes::from("test request body");
        let url = Url::parse("https://api.test.com/messages").unwrap();

        let _ = http.eventsource(&url, None, body.clone()).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let writes = file_writer.get_writes().await;
        assert_eq!(writes.len(), 1, "Should write one file");
        assert_eq!(writes[0].0, debug_path);
        assert_eq!(writes[0].1, body);
    }
}
