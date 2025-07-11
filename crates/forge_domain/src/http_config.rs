use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpConfig {
    pub connect_timeout: u64,
    pub read_timeout: u64,
    pub pool_idle_timeout: u64,
    pub pool_max_idle_per_host: usize,
    pub max_redirects: usize,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: 10,
            read_timeout: 60 * 5, // 5 minutes
            pool_idle_timeout: 90,
            pool_max_idle_per_host: 5,
            max_redirects: 10,
        }
    }
}
