use std::time::Duration;

use anyhow::Context;
use bytes::Bytes;
use forge_domain::{HttpConfig, TlsBackend, TlsVersion};
use forge_services::HttpInfra;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::redirect::Policy;
use reqwest::{Client, Response, StatusCode, Url};
use reqwest_eventsource::{EventSource, RequestBuilderExt};
use tracing::debug;

const VERSION: &str = match option_env!("APP_VERSION") {
    None => env!("CARGO_PKG_VERSION"),
    Some(v) => v,
};

#[derive(Default)]
pub struct ForgeHttpInfra {
    client: Client,
}

fn to_reqwest_tls(tls: TlsVersion) -> reqwest::tls::Version {
    use reqwest::tls::Version;
    match tls {
        TlsVersion::V1_0 => Version::TLS_1_0,
        TlsVersion::V1_1 => Version::TLS_1_1,
        TlsVersion::V1_2 => Version::TLS_1_2,
        TlsVersion::V1_3 => Version::TLS_1_3,
    }
}

impl ForgeHttpInfra {
    pub fn new(config: HttpConfig) -> Self {
        let mut client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(config.connect_timeout))
            .read_timeout(std::time::Duration::from_secs(config.read_timeout))
            .pool_idle_timeout(std::time::Duration::from_secs(config.pool_idle_timeout))
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .redirect(Policy::limited(config.max_redirects))
            .hickory_dns(config.hickory)
            // HTTP/2 configuration from config
            .http2_adaptive_window(config.adaptive_window)
            .http2_keep_alive_interval(config.keep_alive_interval.map(Duration::from_secs))
            .http2_keep_alive_timeout(Duration::from_secs(config.keep_alive_timeout))
            .http2_keep_alive_while_idle(config.keep_alive_while_idle);

        if let Some(version) = config.min_tls_version {
            client = client.min_tls_version(to_reqwest_tls(version));
        }

        if let Some(version) = config.max_tls_version {
            client = client.max_tls_version(to_reqwest_tls(version));
        }

        match config.tls_backend {
            TlsBackend::Rustls => {
                client = client.use_rustls_tls();
            }
            TlsBackend::Default => {}
        }

        Self { client: client.build().unwrap() }
    }

    async fn get(&self, url: &Url, headers: Option<HeaderMap>) -> anyhow::Result<Response> {
        self.execute_request("GET", url, |client| {
            client.get(url.clone()).headers(self.headers(headers))
        })
        .await
    }

    async fn post(&self, url: &Url, body: Bytes) -> anyhow::Result<Response> {
        self.execute_request("POST", url, |client| {
            client
                .post(url.clone())
                .headers(self.headers(None))
                .body(body)
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
    async fn execute_request<F>(
        &self,
        method: &str,
        url: &Url,
        request_builder: F,
    ) -> anyhow::Result<Response>
    where
        F: FnOnce(&Client) -> reqwest::RequestBuilder,
    {
        let response = request_builder(&self.client)
            .send()
            .await
            .with_context(|| format_http_context(None, method, url))?;

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow::anyhow!("HTTP request failed"))
                .with_context(|| format_http_context(Some(status), method, url));
        }

        Ok(response)
    }

    // OpenRouter optional headers ref: https://openrouter.ai/docs/api-reference/overview#headers
    // - `HTTP-Referer`: Identifies your app on openrouter.ai
    // - `X-Title`: Sets/modifies your app's title
    fn headers(&self, headers: Option<HeaderMap>) -> HeaderMap {
        let mut headers = headers.unwrap_or_default();
        headers.insert("User-Agent", HeaderValue::from_static("Forge"));
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

    async fn eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<EventSource> {
        let mut request_headers = self.headers(headers);
        request_headers.insert("Content-Type", HeaderValue::from_static("application/json"));

        self.client
            .post(url.clone())
            .headers(request_headers)
            .body(body)
            .eventsource()
            .with_context(|| format_http_context(None, "POST (EventSource)", url))
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

/// Helper function to format HTTP request/response context for logging and
/// error reporting
fn format_http_context<U: AsRef<str>>(status: Option<StatusCode>, method: &str, url: U) -> String {
    if let Some(status) = status {
        format!("{} {} {}", status.as_u16(), method, url.as_ref())
    } else {
        format!("{} {}", method, url.as_ref())
    }
}

#[async_trait::async_trait]
impl HttpInfra for ForgeHttpInfra {
    async fn get(&self, url: &Url, headers: Option<HeaderMap>) -> anyhow::Result<Response> {
        self.get(url, headers).await
    }

    async fn post(&self, url: &Url, body: Bytes) -> anyhow::Result<Response> {
        self.post(url, body).await
    }

    async fn delete(&self, url: &Url) -> anyhow::Result<Response> {
        self.delete(url).await
    }

    async fn eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<EventSource> {
        self.eventsource(url, headers, body).await
    }
}
