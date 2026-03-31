use std::time::Duration;

use forge_domain::{HttpConfig, TlsBackend, TlsVersion};
use reqwest::Certificate;
use reqwest::redirect::Policy;
use tracing::warn;

fn to_reqwest_tls(tls: TlsVersion) -> reqwest::tls::Version {
    use reqwest::tls::Version;
    match tls {
        TlsVersion::V1_0 => Version::TLS_1_0,
        TlsVersion::V1_1 => Version::TLS_1_1,
        TlsVersion::V1_2 => Version::TLS_1_2,
        TlsVersion::V1_3 => Version::TLS_1_3,
    }
}

/// Extension methods on [`reqwest::ClientBuilder`] that act as composable
/// transformers. Each method applies one configuration concern; callers mix
/// and match them to build any HTTP client without duplicating logic.
pub(crate) trait ClientBuilderExt: Sized {
    /// Applies the full [`HttpConfig`]: timeouts, connection-pooling, redirect
    /// policy, Hickory DNS, HTTP/2 keep-alive, and all TLS settings via
    /// [`ClientBuilderExt::with_tls_config`].
    fn with_http_config(self, config: &HttpConfig) -> Self;

    /// Applies only the TLS subset of [`HttpConfig`]: root certificate paths,
    /// `accept_invalid_certs`, min/max TLS version, and TLS backend selection.
    ///
    /// Certificate file read or parse failures are emitted as warnings and
    /// skipped rather than propagated as errors.
    fn with_tls_config(self, config: &HttpConfig) -> Self;

    /// Routes HTTPS traffic through `HTTP_PROXY` when no `HTTPS_PROXY` or
    /// `ALL_PROXY` is set in the environment. This compensates for reqwest's
    /// intentional behaviour of applying `HTTP_PROXY` only to plaintext HTTP
    /// requests, which causes HTTPS traffic to bypass the proxy in corporate
    /// environments where only `HTTP_PROXY` is configured.
    fn with_proxy_fallback(self) -> anyhow::Result<Self>;

    /// Adds every `(key, value)` pair from `headers` to the client's default
    /// header map. The call is a no-op when the iterator is empty. Returns an
    /// error if any header name or value is invalid.
    fn with_custom_headers<K, V>(
        self,
        headers: impl IntoIterator<Item = (K, V)>,
    ) -> anyhow::Result<Self>
    where
        K: AsRef<str>,
        V: AsRef<str>;
}

impl ClientBuilderExt for reqwest::ClientBuilder {
    fn with_http_config(self, config: &HttpConfig) -> Self {
        self.connect_timeout(Duration::from_secs(config.connect_timeout))
            .read_timeout(Duration::from_secs(config.read_timeout))
            .pool_idle_timeout(Duration::from_secs(config.pool_idle_timeout))
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .redirect(Policy::limited(config.max_redirects))
            .hickory_dns(config.hickory)
            .http2_adaptive_window(config.adaptive_window)
            .http2_keep_alive_interval(config.keep_alive_interval.map(Duration::from_secs))
            .http2_keep_alive_timeout(Duration::from_secs(config.keep_alive_timeout))
            .http2_keep_alive_while_idle(config.keep_alive_while_idle)
            .with_tls_config(config)
    }

    fn with_tls_config(self, config: &HttpConfig) -> Self {
        let mut builder = self;

        if let Some(ref cert_paths) = config.root_cert_paths {
            for cert_path in cert_paths {
                match std::fs::read(cert_path) {
                    Ok(buf) => {
                        if let Ok(cert) = Certificate::from_pem(&buf) {
                            builder = builder.add_root_certificate(cert);
                        } else if let Ok(cert) = Certificate::from_der(&buf) {
                            builder = builder.add_root_certificate(cert);
                        } else {
                            warn!(
                                cert = %cert_path,
                                "Failed to parse certificate as PEM or DER format"
                            );
                        }
                    }
                    Err(error) => {
                        warn!(cert = %cert_path, %error, "Failed to read certificate file");
                    }
                }
            }
        }

        if config.accept_invalid_certs {
            builder = builder.danger_accept_invalid_certs(true);
        }

        if let Some(version) = config.min_tls_version.clone() {
            builder = builder.min_tls_version(to_reqwest_tls(version));
        }

        if let Some(version) = config.max_tls_version.clone() {
            builder = builder.max_tls_version(to_reqwest_tls(version));
        }

        match &config.tls_backend {
            TlsBackend::Rustls => builder.use_rustls_tls(),
            TlsBackend::Default => builder,
        }
    }

    fn with_proxy_fallback(self) -> anyhow::Result<Self> {
        let has_https_proxy = std::env::var("HTTPS_PROXY")
            .or_else(|_| std::env::var("https_proxy"))
            .or_else(|_| std::env::var("ALL_PROXY"))
            .or_else(|_| std::env::var("all_proxy"))
            .is_ok();

        if !has_https_proxy
            && let Ok(proxy_url) =
                std::env::var("HTTP_PROXY").or_else(|_| std::env::var("http_proxy"))
        {
            return Ok(self.proxy(
                reqwest::Proxy::all(&proxy_url)
                    .map_err(|e| anyhow::anyhow!("Invalid HTTP_PROXY URL '{proxy_url}': {e}"))?,
            ));
        }

        Ok(self)
    }

    fn with_custom_headers<K, V>(
        self,
        headers: impl IntoIterator<Item = (K, V)>,
    ) -> anyhow::Result<Self>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut header_map = reqwest::header::HeaderMap::new();

        for (key, value) in headers {
            let k = key.as_ref();
            let v = value.as_ref();
            let header_name = reqwest::header::HeaderName::try_from(k)
                .map_err(|e| anyhow::anyhow!("Invalid header name '{k}': {e}"))?;
            let header_value = reqwest::header::HeaderValue::try_from(v)
                .map_err(|e| anyhow::anyhow!("Invalid header value for '{k}': {e}"))?;
            header_map.insert(header_name, header_value);
        }

        if header_map.is_empty() {
            return Ok(self);
        }

        Ok(self.default_headers(header_map))
    }
}
