use std::time::Duration;

use merge::Merge;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateFrequency {
    Daily,
    Weekly,
    Always,
}

impl Into<Duration> for UpdateFrequency {
    fn into(self) -> Duration {
        match self {
            UpdateFrequency::Daily => Duration::from_secs(60 * 60 * 24), // 1 day
            UpdateFrequency::Weekly => Duration::from_secs(60 * 60 * 24 * 7), // 1 week
            UpdateFrequency::Always => Duration::ZERO,                   // one time,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Merge, Default)]
pub struct Update {
    pub check_frequency: Option<UpdateFrequency>,
    pub auto_update: Option<bool>,
}
