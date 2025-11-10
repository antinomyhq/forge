use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use bytes::Bytes;
use forge_app::HttpInfra;
use forge_domain::{Environment, TlsBackend, TlsVersion};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::redirect::Policy;
use reqwest::{Certificate, Client, Response, StatusCode, Url};
use reqwest_eventsource::{EventSource, RequestBuilderExt};
use tracing::{debug, warn};

const VERSION: &str = match option_env!("APP_VERSION") {
    None => env!("CARGO_PKG_VERSION"),
    Some(v) => v,
};

pub struct ForgeHttpInfra<F> {
    client: Client,
    env: Environment,
    file: Arc<F>,
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

impl<F> ForgeHttpInfra<F> {
    pub fn new(env: Environment, file_writer: Arc<F>) -> Self {
        let env = env.clone();
        let env_http = env.clone();
        let mut client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(
                env_http.http.connect_timeout,
            ))
            .read_timeout(std::time::Duration::from_secs(env_http.http.read_timeout))
            .pool_idle_timeout(std::time::Duration::from_secs(
                env_http.http.pool_idle_timeout,
            ))
            .pool_max_idle_per_host(env_http.http.pool_max_idle_per_host)
            .redirect(Policy::limited(env_http.http.max_redirects))
            .hickory_dns(env_http.http.hickory)
            // HTTP/2 configuration from config
            .http2_adaptive_window(env_http.http.adaptive_window)
            .http2_keep_alive_interval(env_http.http.keep_alive_interval.map(Duration::from_secs))
            .http2_keep_alive_timeout(Duration::from_secs(env_http.http.keep_alive_timeout))
            .http2_keep_alive_while_idle(env_http.http.keep_alive_while_idle);

        // Add root certificates from config
        if let Some(ref cert_paths) = env_http.http.root_cert_paths {
            for cert_path in cert_paths {
                match fs::read(cert_path) {
                    Ok(buf) => {
                        if let Ok(cert) = Certificate::from_pem(&buf) {
                            client = client.add_root_certificate(cert);
                        } else if let Ok(cert) = Certificate::from_der(&buf) {
                            client = client.add_root_certificate(cert);
                        } else {
                            warn!(
                                "Failed to parse certificate as PEM or DER format, cert = {}",
                                cert_path
                            );
                        }
                    }
                    Err(error) => {
                        warn!(
                            "Failed to read certificate file, path = {}, error = {}",
                            cert_path, error
                        );
                    }
                }
            }
        }

        if env_http.http.accept_invalid_certs {
            client = client.danger_accept_invalid_certs(true);
        }

        if let Some(version) = env_http.http.min_tls_version {
            client = client.min_tls_version(to_reqwest_tls(version));
        }

        if let Some(version) = env_http.http.max_tls_version {
            client = client.max_tls_version(to_reqwest_tls(version));
        }

        match env_http.http.tls_backend {
            TlsBackend::Rustls => {
                client = client.use_rustls_tls();
            }
            TlsBackend::Default => {}
        }

        Self { env, client: client.build().unwrap(), file: file_writer }
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
    async fn execute_request<B>(
        &self,
        method: &str,
        url: &Url,
        request_builder: B,
    ) -> anyhow::Result<Response>
    where
        B: FnOnce(&Client) -> reqwest::RequestBuilder,
    {
        let response = request_builder(&self.client)
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

impl<F: forge_app::FileWriterInfra + 'static> ForgeHttpInfra<F> {
    async fn eventsource(
        &self,
        url: &Url,
        headers: Option<HeaderMap>,
        body: Bytes,
    ) -> anyhow::Result<EventSource> {
        let mut request_headers = self.headers(headers);
        request_headers.insert("Content-Type", HeaderValue::from_static("application/json"));

        if self.env.debug_requests {
            let file_writer = self.file.clone();
            let body_clone = body.clone();
            tokio::spawn(async move {
                let debug_path = PathBuf::from(".forge/request.body.json");
                let _ = file_writer.write(&debug_path, body_clone).await;
            });
        }

        self.client
            .post(url.clone())
            .headers(request_headers)
            .body(body)
            .eventsource()
            .with_context(|| format_http_context(None, "POST (EventSource)", url))
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
impl<F: forge_app::FileWriterInfra + 'static> HttpInfra for ForgeHttpInfra<F> {
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
