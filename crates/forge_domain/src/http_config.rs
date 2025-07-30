use serde::{Deserialize, Serialize};
/// Supported TLS versions for client connections
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TlsVersion {
    #[default]
    Auto,
    Tls10,
    Tls11,
    Tls12,
    Tls13,
}

impl std::fmt::Display for TlsVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsVersion::Auto => write!(f, "auto"),
            TlsVersion::Tls10 => write!(f, "tls1.0"),
            TlsVersion::Tls11 => write!(f, "tls1.1"),
            TlsVersion::Tls12 => write!(f, "tls1.2"),
            TlsVersion::Tls13 => write!(f, "tls1.3"),
        }
    }
}

impl std::str::FromStr for TlsVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(TlsVersion::Auto),
            "tls1.0" | "tls10" | "1.0" => Ok(TlsVersion::Tls10),
            "tls1.1" | "tls11" | "1.1" => Ok(TlsVersion::Tls11),
            "tls1.2" | "tls12" | "1.2" => Ok(TlsVersion::Tls12),
            "tls1.3" | "tls13" | "1.3" => Ok(TlsVersion::Tls13),
            _ => Err(format!(
                "Invalid TLS version: {s}. Valid options are: auto, tls1.0, tls1.1, tls1.2, tls1.3"
            )),
        }
    }
}


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TlsBackend {
    #[default]
    Default,
    Native,
    Rustls,
}

impl std::fmt::Display for TlsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsBackend::Default => write!(f, "default"),
            TlsBackend::Native => write!(f, "native"),
            TlsBackend::Rustls => write!(f, "rustls"),
        }
    }
}

impl std::str::FromStr for TlsBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(TlsBackend::Default),
            "native" => Ok(TlsBackend::Native),
            "rustls" => Ok(TlsBackend::Rustls),
            _ => Err(format!(
                "Invalid TLS backend: {s}. Valid options are: default, native, rustls"
            )),
        }
    }
}

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
    pub min_tls_version: TlsVersion,
    pub max_tls_version: TlsVersion,
    pub tls_fallback_enabled: bool,
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
            min_tls_version: TlsVersion::Tls12, // Minimum secure version
            max_tls_version: TlsVersion::Auto,  // Try newest available
            tls_fallback_enabled: true,         // Enable automatic fallback
        }
    }
}
