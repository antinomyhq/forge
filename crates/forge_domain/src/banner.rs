use merge::Merge;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for banner display behavior
#[derive(Debug, Clone, JsonSchema, PartialEq)]
pub enum BannerConfig {
    /// Show the default banner (when not specified)
    #[serde(skip)]
    None,
    /// Disable banner display
    Disabled,
    /// Use a custom banner from file path
    Custom(PathBuf),
}

impl Serialize for BannerConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            BannerConfig::None => {
                // This shouldn't be serialized due to skip attribute, but just in case
                serializer.serialize_none()
            }
            BannerConfig::Disabled => serializer.serialize_str("disabled"),
            BannerConfig::Custom(path) => serializer.serialize_str(&path.to_string_lossy()),
        }
    }
}

impl<'de> Deserialize<'de> for BannerConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "disabled" {
            Ok(BannerConfig::Disabled)
        } else {
            Ok(BannerConfig::Custom(PathBuf::from(s)))
        }
    }
}

impl Merge for BannerConfig {
    fn merge(&mut self, other: Self) {
        *self = other;
    }
}
