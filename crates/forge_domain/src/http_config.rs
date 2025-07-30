use serde::{Deserialize, Serialize};

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
        }
    }
}
