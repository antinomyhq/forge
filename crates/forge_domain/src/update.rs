use std::str::FromStr;
use std::time::Duration;

use derive_setters::Setters;
use merge::Merge;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdateFrequency {
    Daily,
    Weekly,
    #[default]
    Always,
}

impl FromStr for UpdateFrequency {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "daily" => Ok(UpdateFrequency::Daily),
            "weekly" => Ok(UpdateFrequency::Weekly),
            "always" => Ok(UpdateFrequency::Always),
            _ => Err(format!("Unknown update frequency: {}", s)),
        }
    }
}

impl From<UpdateFrequency> for Duration {
    fn from(val: UpdateFrequency) -> Self {
        match val {
            UpdateFrequency::Daily => Duration::from_secs(60 * 60 * 24), // 1 day
            UpdateFrequency::Weekly => Duration::from_secs(60 * 60 * 24 * 7), // 1 week
            UpdateFrequency::Always => Duration::ZERO,                   // one time,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Merge, Default, JsonSchema, Setters)]
#[merge(strategy = merge::option::overwrite_none)]
pub struct Update {
    pub frequency: Option<UpdateFrequency>,
    pub auto_update: Option<bool>,
}
