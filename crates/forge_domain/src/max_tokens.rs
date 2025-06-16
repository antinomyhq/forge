use std::fmt;
use std::ops::Deref;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A newtype for max_tokens values with built-in validation
///
/// Max tokens controls the maximum number of tokens the model can generate:
/// - Lower values (e.g., 100) limit response length for concise outputs
/// - Higher values (e.g., 4000) allow for longer, more detailed responses
/// - Valid range is 1 to 100,000 (reasonable upper bound for most models)
/// - If not specified, the model provider's default will be used
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, JsonSchema)]
pub struct MaxTokens(u32);

impl MaxTokens {
    /// Creates a new MaxTokens value, returning an error if outside the valid
    /// range (1 to 100,000)
    pub fn new(value: u32) -> Result<Self, String> {
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(format!(
                "max_tokens must be between 1 and 100000, got {value}"
            ))
        }
    }

    /// Creates a new MaxTokens value without validation
    ///
    /// # Safety
    /// This function should only be used when the value is known to be valid
    pub fn new_unchecked(value: u32) -> Self {
        debug_assert!(Self::is_valid(value), "invalid max_tokens: {value}");
        Self(value)
    }

    /// Returns true if the max_tokens value is within the valid range (1 to
    /// 100,000)
    pub fn is_valid(value: u32) -> bool {
        (1..=100_000).contains(&value)
    }

    /// Returns the inner u32 value
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl Deref for MaxTokens {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<MaxTokens> for u32 {
    fn from(max_tokens: MaxTokens) -> Self {
        max_tokens.0
    }
}

impl fmt::Display for MaxTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for MaxTokens {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for MaxTokens {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let value = u32::deserialize(deserializer)?;
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(Error::custom(format!(
                "max_tokens must be between 1 and 100000, got {value}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_max_tokens_creation() {
        // Valid max_tokens values should be created successfully
        let valid_values = [1, 100, 1000, 4000, 8000, 100_000];
        for value in valid_values {
            let result = MaxTokens::new(value);
            assert!(result.is_ok(), "MaxTokens {value} should be valid");
            assert_eq!(result.unwrap().value(), value);
        }

        // Invalid max_tokens values should return an error
        let invalid_values = [0, 100_001, 200_000, 1_000_000];
        for value in invalid_values {
            let result = MaxTokens::new(value);
            assert!(result.is_err(), "MaxTokens {value} should be invalid");
            assert!(
                result
                    .unwrap_err()
                    .contains("max_tokens must be between 1 and 100000"),
                "Error should mention valid range"
            );
        }
    }

    #[test]
    fn test_max_tokens_serialization() {
        let max_tokens = MaxTokens::new(4000).unwrap();
        let json = serde_json::to_value(max_tokens).unwrap();

        if let serde_json::Value::Number(num) = &json {
            let int_val = num.as_u64().unwrap();
            assert_eq!(int_val, 4000);
        } else {
            panic!("Expected a number, got {json:?}");
        }
    }

    #[test]
    fn test_max_tokens_deserialization() {
        // Valid max_tokens values should deserialize correctly
        let valid_values = [1, 100, 1000, 4000, 8000, 100_000];
        for value in valid_values {
            let json = json!(value);
            let max_tokens: Result<MaxTokens, _> = serde_json::from_value(json);
            assert!(
                max_tokens.is_ok(),
                "Valid max_tokens {value} should deserialize"
            );
            assert_eq!(max_tokens.unwrap().value(), value);
        }

        // Invalid max_tokens values should fail deserialization
        let invalid_values = [0, 100_001, 200_000, 1_000_000];
        for value in invalid_values {
            let json = json!(value);
            let max_tokens: Result<MaxTokens, _> = serde_json::from_value(json);
            assert!(
                max_tokens.is_err(),
                "Invalid max_tokens {value} should fail deserialization"
            );
            let err = max_tokens.unwrap_err().to_string();
            assert!(
                err.contains("max_tokens must be between 1 and 100000"),
                "Error should mention valid range: {err}"
            );
        }
    }

    #[test]
    fn test_max_tokens_in_struct() {
        #[derive(Serialize, Deserialize, Debug)]
        struct TestStruct {
            max_tokens: MaxTokens,
        }

        // Valid max_tokens
        let json = json!({
            "max_tokens": 4000
        });
        let test_struct: Result<TestStruct, _> = serde_json::from_value(json);
        assert!(test_struct.is_ok());
        assert_eq!(test_struct.unwrap().max_tokens.value(), 4000);

        // Invalid max_tokens
        let json = json!({
            "max_tokens": 200_000
        });
        let test_struct: Result<TestStruct, _> = serde_json::from_value(json);
        assert!(test_struct.is_err());
        let err = test_struct.unwrap_err().to_string();
        assert!(
            err.contains("max_tokens must be between 1 and 100000"),
            "Error should mention valid range: {err}"
        );
    }
}
