use serde::{Deserialize, Serialize};
use strum_macros::EnumString;

/// TLS version enum for configuring minimum and maximum TLS protocol versions.
///
/// Used with `HttpConfig` to specify TLS version constraints for HTTP
/// connections.
///
/// # Example
/// ```
/// use forge_domain::{HttpConfig, TlsVersion, TlsBackend};
///
/// let config = HttpConfig {
///     min_tls_version: Some(TlsVersion::V1_2),
///     max_tls_version: Some(TlsVersion::V1_3),
///     tls_backend: TlsBackend::Rustls,
///     ..HttpConfig::default()
/// };
/// ```
///
/// # Environment Variables
/// - `FORGE_HTTP_MIN_TLS_VERSION`: Set minimum TLS version (e.g., "1.2")
/// - `FORGE_HTTP_MAX_TLS_VERSION`: Set maximum TLS version (e.g., "1.3")
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "camelCase")]
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
/// redirects, DNS resolution, and TLS settings.
///
/// # TLS Configuration
/// The `min_tls_version` and `max_tls_version` fields allow you to specify
/// TLS protocol version constraints. These are optional and when `None`,
/// the TLS library defaults will be used.
///
/// # Example
/// ```
/// use forge_domain::{HttpConfig, TlsVersion, TlsBackend};
///
/// let config = HttpConfig {
///     connect_timeout: 30,
///     min_tls_version: Some(TlsVersion::V1_2),
///     max_tls_version: Some(TlsVersion::V1_3),
///     tls_backend: TlsBackend::Rustls,
///     ..HttpConfig::default()
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: 30, // 30 seconds
            read_timeout: 900,   /* 15 minutes; this should be in sync with the server function
                                  * execution timeout */
            pool_idle_timeout: 90,
            pool_max_idle_per_host: 5,
            max_redirects: 10,
            hickory: false,
            tls_backend: TlsBackend::default(),
            min_tls_version: None, // Use TLS library default
            max_tls_version: None, // Use TLS library default
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_tls_version_from_str() {
        assert_eq!(TlsVersion::from_str("1.0").unwrap(), TlsVersion::V1_0);
        assert_eq!(TlsVersion::from_str("1.1").unwrap(), TlsVersion::V1_1);
        assert_eq!(TlsVersion::from_str("1.2").unwrap(), TlsVersion::V1_2);
        assert_eq!(TlsVersion::from_str("1.3").unwrap(), TlsVersion::V1_3);

        assert!(TlsVersion::from_str("invalid").is_err());
        assert!(TlsVersion::from_str("2.0").is_err());
    }

    #[test]
    fn test_tls_version_display() {
        assert_eq!(TlsVersion::V1_0.to_string(), "1.0");
        assert_eq!(TlsVersion::V1_1.to_string(), "1.1");
        assert_eq!(TlsVersion::V1_2.to_string(), "1.2");
        assert_eq!(TlsVersion::V1_3.to_string(), "1.3");
    }

    #[test]
    fn test_tls_version_default() {
        assert_eq!(TlsVersion::default(), TlsVersion::V1_3);
    }

    #[test]
    fn test_http_config_with_tls_versions() {
        let config = HttpConfig {
            min_tls_version: Some(TlsVersion::V1_2),
            max_tls_version: Some(TlsVersion::V1_3),
            ..HttpConfig::default()
        };

        assert_eq!(config.min_tls_version, Some(TlsVersion::V1_2));
        assert_eq!(config.max_tls_version, Some(TlsVersion::V1_3));
    }
}
