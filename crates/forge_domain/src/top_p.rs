use std::fmt;
use std::ops::Deref;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A newtype for top_p values with built-in validation
///
/// Top-p (nucleus sampling) controls the diversity of the model's output:
/// - Lower values (e.g., 0.1) make responses more focused by considering only
///   the most probable tokens
/// - Higher values (e.g., 0.9) make responses more diverse by considering a
///   broader range of tokens
/// - Valid range is 0.0 to 1.0
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, JsonSchema)]
pub struct TopP(f32);

impl TopP {
    /// Creates a new TopP value, returning an error if outside the valid
    /// range (0.0 to 1.0)
    pub fn new(value: f32) -> Result<Self, String> {
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(format!("top_p must be between 0.0 and 1.0, got {value}"))
        }
    }

    /// Creates a new TopP value without validation
    ///
    /// # Safety
    /// This function should only be used when the value is known to be valid
    pub fn new_unchecked(value: f32) -> Self {
        debug_assert!(Self::is_valid(value), "invalid top_p: {value}");
        Self(value)
    }

    /// Returns true if the top_p value is within the valid range (0.0 to 1.0)
    pub fn is_valid(value: f32) -> bool {
        (0.0..=1.0).contains(&value)
    }

    /// Returns the inner f32 value
    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Deref for TopP {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<TopP> for f32 {
    fn from(top_p: TopP) -> Self {
        top_p.0
    }
}

impl fmt::Display for TopP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for TopP {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert to string with fixed precision to avoid floating point issues
        // and then parse back to ensure consistent serialization
        let formatted = format!("{:.2}", self.0);
        let value = formatted.parse::<f32>().unwrap();
        serializer.serialize_f32(value)
    }
}

impl<'de> Deserialize<'de> for TopP {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let value = f32::deserialize(deserializer)?;
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(Error::custom(format!(
                "top_p must be between 0.0 and 1.0, got {value}"
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
    fn test_top_p_creation() {
        // Valid top_p values should be created successfully
        let valid_values = [0.0, 0.1, 0.5, 0.9, 1.0];
        for value in valid_values {
            let result = TopP::new(value);
            assert!(result.is_ok(), "TopP {value} should be valid");
            assert_eq!(result.unwrap().value(), value);
        }

        // Invalid top_p values should return an error
        let invalid_values = [-0.1, 1.1, 2.0, -1.0, 10.0];
        for value in invalid_values {
            let result = TopP::new(value);
            assert!(result.is_err(), "TopP {value} should be invalid");
            assert!(
                result
                    .unwrap_err()
                    .contains("top_p must be between 0.0 and 1.0"),
                "Error should mention valid range"
            );
        }
    }

    #[test]
    fn test_top_p_serialization() {
        let top_p = TopP::new(0.7).unwrap();
        let json = serde_json::to_value(top_p).unwrap();

        // When serializing floating point numbers, precision issues might occur
        // So we'll check if the serialized value is approximately equal to 0.7
        if let serde_json::Value::Number(num) = &json {
            let float_val = num.as_f64().unwrap();
            assert!(
                (float_val - 0.7).abs() < 0.001,
                "Expected approximately 0.7, got {float_val}"
            );
        } else {
            panic!("Expected a number, got {json:?}");
        }
    }

    #[test]
    fn test_top_p_deserialization() {
        // Valid top_p values should deserialize correctly
        let valid_values = [0.0, 0.1, 0.5, 0.9, 1.0];
        for value in valid_values {
            let json = json!(value);
            let top_p: Result<TopP, _> = serde_json::from_value(json);
            assert!(top_p.is_ok(), "Valid top_p {value} should deserialize");
            assert_eq!(top_p.unwrap().value(), value);
        }

        // Invalid top_p values should fail deserialization
        let invalid_values = [-0.1, 1.1, 2.0, -1.0, 10.0];
        for value in invalid_values {
            let json = json!(value);
            let top_p: Result<TopP, _> = serde_json::from_value(json);
            assert!(
                top_p.is_err(),
                "Invalid top_p {value} should fail deserialization"
            );
            let err = top_p.unwrap_err().to_string();
            assert!(
                err.contains("top_p must be between 0.0 and 1.0"),
                "Error should mention valid range: {err}"
            );
        }
    }

    #[test]
    fn test_top_p_in_struct() {
        #[derive(Serialize, Deserialize, Debug)]
        struct TestStruct {
            top_p: TopP,
        }

        // Valid top_p
        let json = json!({
            "top_p": 0.7
        });
        let test_struct: Result<TestStruct, _> = serde_json::from_value(json);
        assert!(test_struct.is_ok());
        assert_eq!(test_struct.unwrap().top_p.value(), 0.7);

        // Invalid top_p
        let json = json!({
            "top_p": 1.5
        });
        let test_struct: Result<TestStruct, _> = serde_json::from_value(json);
        assert!(test_struct.is_err());
        let err = test_struct.unwrap_err().to_string();
        assert!(
            err.contains("top_p must be between 0.0 and 1.0"),
            "Error should mention valid range: {err}"
        );
    }
}
