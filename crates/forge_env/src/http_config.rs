use serde::{Deserialize, Serialize};
use strum_macros::EnumString;

/// TLS version enum for configuring minimum and maximum TLS protocol versions.
///
/// Used with `HttpConfig` to specify TLS version constraints for HTTP
/// connections.
///
/// # Example
/// ```no_run
/// use forge_env::{HttpConfig, TlsVersion, TlsBackend};
/// use fake::{Fake, Faker};
///
/// let config = HttpConfig {
///     min_tls_version: Some(TlsVersion::V1_2),
///     max_tls_version: Some(TlsVersion::V1_3),
///     tls_backend: TlsBackend::Rustls,
///     ..Faker.fake()
/// };
/// ```
///
/// # Environment Variables
/// - `FORGE_HTTP_MIN_TLS_VERSION`: Set minimum TLS version (e.g., "1.2")
/// - `FORGE_HTTP_MAX_TLS_VERSION`: Set maximum TLS version (e.g., "1.3")
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, fake::Dummy)]
#[serde(rename_all = "snake_case")]
pub enum TlsVersion {
    #[serde(rename = "1.0")]
    V1_0,
    #[serde(rename = "1.1")]
    V1_1,
    #[serde(rename = "1.2")]
    V1_2,
    #[default]
    #[serde(rename = "1.3")]
    V1_3,
}

impl std::fmt::Display for TlsVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsVersion::V1_0 => write!(f, "1.0"),
            TlsVersion::V1_1 => write!(f, "1.1"),
            TlsVersion::V1_2 => write!(f, "1.2"),
            TlsVersion::V1_3 => write!(f, "1.3"),
        }
    }
}

impl std::str::FromStr for TlsVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1.0" => Ok(TlsVersion::V1_0),
            "1.1" => Ok(TlsVersion::V1_1),
            "1.2" => Ok(TlsVersion::V1_2),
            "1.3" => Ok(TlsVersion::V1_3),
            _ => Err(format!(
                "Invalid TLS version: {s}. Valid options are: 1.0, 1.1, 1.2, 1.3"
            )),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, EnumString, fake::Dummy)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "lowercase")]
pub enum TlsBackend {
    #[default]
    Default,
    Rustls,
}

impl std::fmt::Display for TlsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsBackend::Default => write!(f, "default"),
            TlsBackend::Rustls => write!(f, "rustls"),
        }
    }
}

/// HTTP client configuration with support for timeouts, connection pooling,
/// redirects, DNS resolution, TLS settings, and HTTP/2 configuration.
///
/// # TLS Configuration
/// The `min_tls_version` and `max_tls_version` fields allow you to specify
/// TLS protocol version constraints. These are optional and when `None`,
/// the TLS library defaults will be used.
///
/// # HTTP/2 Configuration
/// The HTTP/2 settings control adaptive window sizing, keep-alive behavior,
/// and connection management for HTTP/2 connections.
///
/// # Environment Variables
/// All HttpConfig fields can be configured via environment variables:
/// - `FORGE_HTTP_CONNECT_TIMEOUT`: Connection timeout in seconds (default: 30)
/// - `FORGE_HTTP_READ_TIMEOUT`: Read timeout in seconds (default: 900)
/// - `FORGE_HTTP_POOL_IDLE_TIMEOUT`: Pool idle timeout in seconds (default: 90)
/// - `FORGE_HTTP_POOL_MAX_IDLE_PER_HOST`: Max idle connections per host
///   (default: 5)
/// - `FORGE_HTTP_MAX_REDIRECTS`: Maximum redirects to follow (default: 10)
/// - `FORGE_HTTP_USE_HICKORY`: Use Hickory DNS resolver (default: false)
/// - `FORGE_HTTP_TLS_BACKEND`: TLS backend ("default" or "rustls", default:
///   "default")
/// - `FORGE_HTTP_MIN_TLS_VERSION`: Minimum TLS version ("1.0", "1.1", "1.2",
///   "1.3")
/// - `FORGE_HTTP_MAX_TLS_VERSION`: Maximum TLS version ("1.0", "1.1", "1.2",
///   "1.3")
/// - `FORGE_HTTP_ADAPTIVE_WINDOW`: Enable HTTP/2 adaptive window (default:
///   true)
/// - `FORGE_HTTP_KEEP_ALIVE_INTERVAL`: Keep-alive interval in seconds (default:
///   60, use "none"/"disabled" to disable)
/// - `FORGE_HTTP_KEEP_ALIVE_TIMEOUT`: Keep-alive timeout in seconds (default:
///   10)
/// - `FORGE_HTTP_KEEP_ALIVE_WHILE_IDLE`: Keep-alive while idle (default: true)
/// - `FORGE_HTTP_ACCEPT_INVALID_CERTS`: Accept invalid certificates (default:
///   false) - USE WITH CAUTION
/// - `FORGE_HTTP_ROOT_CERT_PATHS`: Paths to root certificate files (PEM, CRT,
///   CER format), multiple paths separated by commas
///
/// # Example
/// ```no_run
/// use forge_env::{HttpConfig, TlsVersion, TlsBackend};
/// use fake::{Fake, Faker};
///
/// let config = HttpConfig {
///     connect_timeout: 30,
///     min_tls_version: Some(TlsVersion::V1_2),
///     max_tls_version: Some(TlsVersion::V1_3),
///     tls_backend: TlsBackend::Rustls,
///     adaptive_window: true,
///     keep_alive_interval: Some(60),
///     ..Faker.fake()
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, fake::Dummy)]
#[serde(rename_all = "snake_case")]
pub struct HttpConfig {
    pub connect_timeout: u64,
    pub read_timeout: u64,
    pub pool_idle_timeout: u64,
    pub pool_max_idle_per_host: usize,
    pub max_redirects: usize,
    pub hickory: bool,
    pub tls_backend: TlsBackend,
    /// Minimum TLS protocol version to use. When `None`, uses TLS library
    /// default.
    pub min_tls_version: Option<TlsVersion>,
    /// Maximum TLS protocol version to use. When `None`, uses TLS library
    /// default.
    pub max_tls_version: Option<TlsVersion>,
    /// Adaptive window sizing for improved flow control.
    pub adaptive_window: bool,
    /// Keep-alive interval in seconds. When `None`, keep-alive is
    /// disabled.
    pub keep_alive_interval: Option<u64>,
    /// Keep-alive timeout in seconds.
    pub keep_alive_timeout: u64,
    /// Keep-alive while connection is idle.
    pub keep_alive_while_idle: bool,
    /// Accept invalid certificates. This should be used with caution.
    pub accept_invalid_certs: bool,
    /// Paths to root certificate files (PEM, CRT, CER format). Multiple paths
    /// can be separated by commas.
    pub root_cert_paths: Option<Vec<String>>,
}
