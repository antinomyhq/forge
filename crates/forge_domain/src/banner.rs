use std::path::PathBuf;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Banner configuration options for workflow
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", try_from = "String", into = "String")]
pub enum Banner {
    /// Use a custom banner from a file
    Custom(PathBuf),
    /// Use the default banner
    Default,
    /// Disable the banner
    Disabled,
}

impl FromStr for Banner {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(Banner::Default),
            "disabled" | "disable" | "none" | "off" => Ok(Banner::Disabled),
            path => Ok(Banner::Custom(PathBuf::from(path))),
        }
    }
}

impl From<Banner> for String {
    fn from(banner: Banner) -> Self {
        match banner {
            Banner::Default => "default".to_string(),
            Banner::Disabled => "disabled".to_string(),
            Banner::Custom(path) => path.to_string_lossy().to_string(),
        }
    }
}

impl TryFrom<String> for Banner {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Banner::from_str(&s)
    }
}

impl Banner {
    /// Returns the path if this is a custom banner
    pub fn as_path(&self) -> Option<&PathBuf> {
        match self {
            Banner::Custom(path) => Some(path),
            _ => None,
        }
    }

    /// Returns true if the banner is disabled
    pub fn is_disabled(&self) -> bool {
        matches!(self, Banner::Disabled)
    }

    /// Returns true if the banner is the default
    pub fn is_default(&self) -> bool {
        matches!(self, Banner::Default)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_banner_from_str_default() {
        // Act
        let actual = Banner::from_str("default").unwrap();

        // Assert
        assert_eq!(actual, Banner::Default);
    }

    #[test]
    fn test_banner_from_str_disabled_variants() {
        // Act & Assert
        assert_eq!(Banner::from_str("disabled").unwrap(), Banner::Disabled);
        assert_eq!(Banner::from_str("disable").unwrap(), Banner::Disabled);
        assert_eq!(Banner::from_str("none").unwrap(), Banner::Disabled);
        assert_eq!(Banner::from_str("off").unwrap(), Banner::Disabled);
    }

    #[test]
    fn test_banner_from_str_custom_path() {
        // Act
        let actual = Banner::from_str("./custom-banner.txt").unwrap();

        // Assert
        assert_eq!(actual, Banner::Custom(PathBuf::from("./custom-banner.txt")));
    }

    #[test]
    fn test_banner_serialize_deserialize_default() {
        // Fixture
        let fixture = Banner::Default;

        // Act
        let serialized = serde_json::to_string(&fixture).unwrap();
        let actual: Banner = serde_json::from_str(&serialized).unwrap();

        // Assert
        assert_eq!(actual, Banner::Default);
        assert_eq!(serialized, r#""default""#);
    }

    #[test]
    fn test_banner_serialize_deserialize_disabled() {
        // Fixture
        let fixture = Banner::Disabled;

        // Act
        let serialized = serde_json::to_string(&fixture).unwrap();
        let actual: Banner = serde_json::from_str(&serialized).unwrap();

        // Assert
        assert_eq!(actual, Banner::Disabled);
        assert_eq!(serialized, r#""disabled""#);
    }

    #[test]
    fn test_banner_serialize_deserialize_custom() {
        // Fixture
        let fixture = Banner::Custom(PathBuf::from("./my-banner.txt"));

        // Act
        let serialized = serde_json::to_string(&fixture).unwrap();
        let actual: Banner = serde_json::from_str(&serialized).unwrap();

        // Assert
        assert_eq!(actual, Banner::Custom(PathBuf::from("./my-banner.txt")));
    }

    #[test]
    fn test_banner_deserialize_simple_path() {
        // Fixture - simple string path (user-friendly format)
        let fixture = r#""./my-banner.txt""#;

        // Act
        let actual: Banner = serde_json::from_str(fixture).unwrap();

        // Assert
        assert_eq!(actual, Banner::Custom(PathBuf::from("./my-banner.txt")));
    }

    #[test]
    fn test_banner_yaml_deserialize_string() {
        // Fixture - YAML string format
        let fixture = r#"banner: disabled"#;

        // Act
        let actual: serde_yml::Value = serde_yml::from_str(fixture).unwrap();

        // Assert
        assert_eq!(actual["banner"], "disabled");
    }

    #[test]
    fn test_banner_as_path() {
        // Fixture
        let custom = Banner::Custom(PathBuf::from("./test.txt"));
        let default = Banner::Default;
        let disabled = Banner::Disabled;

        // Act & Assert
        assert_eq!(custom.as_path(), Some(&PathBuf::from("./test.txt")));
        assert_eq!(default.as_path(), None);
        assert_eq!(disabled.as_path(), None);
    }

    #[test]
    fn test_banner_is_disabled() {
        // Fixture
        let disabled = Banner::Disabled;
        let default = Banner::Default;

        // Act & Assert
        assert!(disabled.is_disabled());
        assert!(!default.is_disabled());
    }

    #[test]
    fn test_banner_is_default() {
        // Fixture
        let default = Banner::Default;
        let disabled = Banner::Disabled;

        // Act & Assert
        assert!(default.is_default());
        assert!(!disabled.is_default());
    }
}
