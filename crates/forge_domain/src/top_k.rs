use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A newtype for top_k values with built-in validation
///
/// Top-k controls the number of highest probability vocabulary tokens to keep:
/// - Lower values (e.g., 10) make responses more focused by considering only
///   the top K most likely tokens
/// - Higher values (e.g., 100) make responses more diverse by considering more
///   token options
/// - Valid range is 1 to 1000 (inclusive)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TopK(u32);

impl TopK {
    /// Creates a new TopK value, returning an error if outside the valid
    /// range (1 to 1000)
    pub fn new(value: u32) -> Result<Self, String> {
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(format!("top_k must be between 1 and 1000, got {value}"))
        }
    }

    /// Creates a new TopK value without validation
    ///
    /// # Safety
    /// This function should only be used when the value is known to be valid
    pub fn new_unchecked(value: u32) -> Self {
        debug_assert!(Self::is_valid(value), "invalid top_k: {value}");
        Self(value)
    }

    /// Returns true if the top_k value is within the valid range (1 to 1000)
    pub fn is_valid(value: u32) -> bool {
        (1..=1000).contains(&value)
    }

    /// Returns the inner u32 value
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl Deref for TopK {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<TopK> for u32 {
    fn from(top_k: TopK) -> Self {
        top_k.0
    }
}

impl fmt::Display for TopK {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for TopK {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for TopK {
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
                "top_k must be between 1 and 1000, got {value}"
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
    fn test_top_k_creation() {
        // Valid top_k values should be created successfully
        let valid_values = [1, 10, 50, 100, 500, 1000];
        for value in valid_values {
            let result = TopK::new(value);
            assert!(result.is_ok(), "TopK {value} should be valid");
            assert_eq!(result.unwrap().value(), value);
        }

        // Invalid top_k values should return an error
        let invalid_values = [0, 1001, 2000, 5000];
        for value in invalid_values {
            let result = TopK::new(value);
            assert!(result.is_err(), "TopK {value} should be invalid");
            assert!(
                result
                    .unwrap_err()
                    .contains("top_k must be between 1 and 1000"),
                "Error should mention valid range"
            );
        }
    }

    #[test]
    fn test_top_k_serialization() {
        let top_k = TopK::new(50).unwrap();
        let json = serde_json::to_value(top_k).unwrap();

        if let serde_json::Value::Number(num) = &json {
            let int_val = num.as_u64().unwrap();
            assert_eq!(int_val, 50);
        } else {
            panic!("Expected a number, got {json:?}");
        }
    }

    #[test]
    fn test_top_k_deserialization() {
        // Valid top_k values should deserialize correctly
        let valid_values = [1, 10, 50, 100, 500, 1000];
        for value in valid_values {
            let json = json!(value);
            let top_k: Result<TopK, _> = serde_json::from_value(json);
            assert!(top_k.is_ok(), "Valid top_k {value} should deserialize");
            assert_eq!(top_k.unwrap().value(), value);
        }

        // Invalid top_k values should fail deserialization
        let invalid_values = [0, 1001, 2000, 5000];
        for value in invalid_values {
            let json = json!(value);
            let top_k: Result<TopK, _> = serde_json::from_value(json);
            assert!(
                top_k.is_err(),
                "Invalid top_k {value} should fail deserialization"
            );
            let err = top_k.unwrap_err().to_string();
            assert!(
                err.contains("top_k must be between 1 and 1000"),
                "Error should mention valid range: {err}"
            );
        }
    }

    #[test]
    fn test_top_k_in_struct() {
        #[derive(Serialize, Deserialize, Debug)]
        struct TestStruct {
            top_k: TopK,
        }

        // Valid top_k
        let json = json!({
            "top_k": 50
        });
        let test_struct: Result<TestStruct, _> = serde_json::from_value(json);
        assert!(test_struct.is_ok());
        assert_eq!(test_struct.unwrap().top_k.value(), 50);

        // Invalid top_k
        let json = json!({
            "top_k": 1500
        });
        let test_struct: Result<TestStruct, _> = serde_json::from_value(json);
        assert!(test_struct.is_err());
        let err = test_struct.unwrap_err().to_string();
        assert!(
            err.contains("top_k must be between 1 and 1000"),
            "Error should mention valid range: {err}"
        );
    }
}
