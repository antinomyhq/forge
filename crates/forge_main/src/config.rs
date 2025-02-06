use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use colored::Colorize;
use forge_domain::Environment;

/// Custom error type for configuration-related errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Unknown configuration key: {0}")]
    UnknownKey(String),
    #[error("Model name cannot be empty")]
    EmptyModelName,
    #[error("Tool timeout must be greater than zero")]
    NonPositiveTimeout,
    #[error("Failed to parse timeout value: {0}")]
    MalformedTimeout(String),
}

/// Represents configuration keys available in the system
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConfigKey {
    /// Primary language model to use for main operations
    PrimaryModel,
    /// Secondary language model for fallback or specialized tasks
    SecondaryModel,
    /// Timeout duration for tool operations in seconds
    ToolTimeout,
}

impl ConfigKey {
    /// Returns the string representation of the configuration key
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigKey::PrimaryModel => "primary-model",
            ConfigKey::SecondaryModel => "secondary-model",
            ConfigKey::ToolTimeout => "tool-timeout",
        }
    }
}

impl Display for ConfigKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ConfigKey {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "primary-model" => Ok(ConfigKey::PrimaryModel),
            "secondary-model" => Ok(ConfigKey::SecondaryModel),
            "tool-timeout" => Ok(ConfigKey::ToolTimeout),
            _ => Err(ConfigError::UnknownKey(s.to_string())),
        }
    }
}

/// Represents configuration values with their specific types
#[derive(Debug, Clone)]
pub enum ConfigValue {
    /// Model identifier string
    Model(String),
    ToolTimeout(u64),
}

impl ConfigValue {
    /// Returns the string representation of the configuration value
    pub fn as_str(&self) -> String {
        match self {
            ConfigValue::Model(model) => model.clone(),
            ConfigValue::ToolTimeout(timeout) => timeout.to_string(),
        }
    }

    /// Creates a new ConfigValue from a key-value pair
    pub fn from_key_value(key: &ConfigKey, value: &str) -> Result<Self, ConfigError> {
        match key {
            ConfigKey::PrimaryModel | ConfigKey::SecondaryModel => {
                if value.trim().is_empty() {
                    Err(ConfigError::EmptyModelName)
                } else {
                    Ok(ConfigValue::Model(value.to_string()))
                }
            }
            ConfigKey::ToolTimeout => match value.parse::<u64>() {
                Ok(0) => Err(ConfigError::NonPositiveTimeout),
                Ok(timeout) => Ok(ConfigValue::ToolTimeout(timeout)),
                Err(_) => Err(ConfigError::MalformedTimeout(value.to_string())),
            },
        }
    }
}

impl Display for ConfigValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Main configuration structure holding all config values
#[derive(Default)]
pub struct Config {
    values: HashMap<ConfigKey, ConfigValue>,
}

impl From<&Environment> for Config {
    fn from(env: &Environment) -> Self {
        let mut values = HashMap::new();
        values.insert(
            ConfigKey::PrimaryModel,
            ConfigValue::Model(env.large_model_id.clone()),
        );
        values.insert(
            ConfigKey::SecondaryModel,
            ConfigValue::Model(env.small_model_id.clone()),
        );
        values.insert(ConfigKey::ToolTimeout, ConfigValue::ToolTimeout(300));
        Self { values }
    }
}

impl Config {
    pub fn tool_timeout(&self) -> Option<u64> {
        self.values.get(&ConfigKey::ToolTimeout).and_then(|v| {
            if let ConfigValue::ToolTimeout(timeout) = v {
                Some(*timeout)
            } else {
                None
            }
        })
    }

    pub fn primary_model(&self) -> Option<String> {
        self.get_model(&ConfigKey::PrimaryModel)
    }

    /// Helper method to get model configuration
    fn get_model(&self, key: &ConfigKey) -> Option<String> {
        self.values.get(key).and_then(|v| match v {
            ConfigValue::Model(m) => Some(m.clone()),
            _ => None,
        })
    }

    /// Gets a configuration value by key string
    pub fn get(&self, key: &str) -> Option<String> {
        key.parse::<ConfigKey>()
            .ok()
            .and_then(|k| self.values.get(&k))
            .map(|v| v.as_str())
    }

    /// Inserts a new configuration value
    pub fn insert(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        let config_key = ConfigKey::from_str(key)?;
        let config_value = ConfigValue::from_key_value(&config_key, value)?;
        self.values.insert(config_key, config_value);
        Ok(())
    }

    /// Checks if the configuration is empty
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns a formatted string representation of the configuration
    pub fn to_display_string(&self) -> String {
        let mut output = String::new();
        if self.is_empty() {
            output.push_str(&format!("{}\n", "No configurations set".italic().yellow()));
        } else {
            output.push('\n');
            let mut configs: Vec<_> = self.values.iter().collect();
            configs.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str())); // Sort by key string
            for (key, value) in configs {
                output.push_str(&format!(
                    "{}: {}\n",
                    key.as_str().bold().yellow(),
                    value.as_str().white()
                ));
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_key_from_str() {
        assert_eq!(
            ConfigKey::from_str("primary-model").unwrap(),
            ConfigKey::PrimaryModel
        );
        assert_eq!(
            ConfigKey::from_str("secondary-model").unwrap(),
            ConfigKey::SecondaryModel
        );
        assert_eq!(
            ConfigKey::from_str("tool-timeout").unwrap(),
            ConfigKey::ToolTimeout
        );

        let err = ConfigKey::from_str("invalid-key").unwrap_err();
        assert!(matches!(err, ConfigError::UnknownKey(_)));
    }

    #[test]
    fn test_config_key_as_str() {
        assert_eq!(ConfigKey::PrimaryModel.as_str(), "primary-model");
        assert_eq!(ConfigKey::SecondaryModel.as_str(), "secondary-model");
        assert_eq!(ConfigKey::ToolTimeout.as_str(), "tool-timeout");
    }

    #[test]
    fn test_config_basic() {
        let mut config = Config::default();
        assert!(config.is_empty());

        // Test setting and getting values
        config.insert("primary-model", "gpt-4").unwrap();
        assert_eq!(config.get("primary-model").unwrap(), "gpt-4");

        config.insert("tool-timeout", "30").unwrap();
        assert_eq!(config.get("tool-timeout").unwrap(), "30");

        // Test type-safe accessors
        assert_eq!(config.primary_model().unwrap(), "gpt-4");

        // Test overwriting values
        config.insert("primary-model", "gpt-3.5-turbo").unwrap();
        assert_eq!(config.primary_model().unwrap(), "gpt-3.5-turbo");

        // Test getting non-existent key
        assert!(config.get("non-existent").is_none());

        // Test invalid operations
        assert!(matches!(
            config.insert("invalid-key", "value").unwrap_err(),
            ConfigError::UnknownKey(_)
        ));
        assert!(matches!(
            config.insert("tool-timeout", "invalid").unwrap_err(),
            ConfigError::MalformedTimeout(_)
        ));
        assert!(matches!(
            config.insert("tool-timeout", "0").unwrap_err(),
            ConfigError::NonPositiveTimeout
        ));
    }
}
