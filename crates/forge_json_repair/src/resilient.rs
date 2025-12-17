use serde::Deserialize;

use crate::{json_repair, JsonRepairError};

/// Deserializes JSON from a string with automatic repair on failure.
/// Drop-in replacement for `serde_json::from_str` that attempts JSON repair
/// when normal parsing fails.
pub fn from_str<T>(s: &str) -> Result<T, JsonRepairError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str::<T>(s)
        .map_err(JsonRepairError::from)
        .or_else(|_| {
            tracing::warn!("JSON parsing failed, attempting repair...");
            json_repair::<T>(s)
                .inspect(|_| {
                    tracing::info!("JSON repair successful");
                })
                .inspect_err(|e| {
                    tracing::error!(error = %e, "JSON repair failed");
                })
        })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, PartialEq, Default)]
    struct TestConfig {
        #[serde(default)]
        name: String,
        #[serde(default)]
        value: i32,
    }

    #[test]
    fn test_from_str_valid_json() {
        let fixture = r#"{"name": "test", "value": 42}"#;
        let actual: TestConfig = from_str(fixture).unwrap();
        let expected = TestConfig { name: "test".to_string(), value: 42 };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_str_repairable_json() {
        let fixture = r#"{"name": "test", "value": 42"#;
        let actual: TestConfig = from_str(fixture).unwrap();
        let expected = TestConfig { name: "test".to_string(), value: 42 };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_str_trailing_comma() {
        let fixture = r#"{"name": "test", "value": 42,}"#;
        let actual: TestConfig = from_str(fixture).unwrap();
        let expected = TestConfig { name: "test".to_string(), value: 42 };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_str_with_unwrap_or_default() {
        let fixture = "";
        let actual: TestConfig = from_str(fixture).unwrap_or_default();
        let expected = TestConfig::default();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_str_vec() {
        let fixture = r#"[1, 2, 3]"#;
        let actual: Vec<i32> = from_str(fixture).unwrap();
        let expected = vec![1, 2, 3];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_str_vec_repairable() {
        let fixture = r#"[1, 2, 3"#;
        let actual: Vec<i32> = from_str(fixture).unwrap();
        let expected = vec![1, 2, 3];
        assert_eq!(actual, expected);
    }
}
