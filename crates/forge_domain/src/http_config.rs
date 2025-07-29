use derive_more::Display;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Display)]
#[serde(rename_all = "camelCase")]
pub enum TlsMode {
    #[default]
    Rustls,
    NativeTls,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpConfig {
    pub connect_timeout: u64,
    pub read_timeout: u64,
    pub pool_idle_timeout: u64,
    pub pool_max_idle_per_host: usize,
    pub max_redirects: usize,
    pub tls_mode: TlsMode,
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
            tls_mode: TlsMode::Rustls,
        }
    }
}
