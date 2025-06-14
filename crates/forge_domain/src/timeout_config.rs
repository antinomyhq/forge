use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeoutConfig {
    pub read_timeout: Option<u64>,
    pub pool_idle_timeout: Option<u64>,
    pub pool_max_idle_per_host: Option<usize>,
    pub max_redirects: Option<usize>,
}
