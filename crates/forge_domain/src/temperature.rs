use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A newtype for temperature values with built-in validation
///
/// Temperature controls the randomness in the model's output:
/// - Lower values (e.g., 0.1) make responses more focused, deterministic, and
///   coherent
/// - Higher values (e.g., 0.8) make responses more creative, diverse, and
///   exploratory
/// - Valid range is 0.0 to 2.0
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Temperature(f32);

impl Temperature {
    /// Creates a new Temperature value, returning an error if outside the valid
    /// range (0.0 to 2.0)
    pub fn new(value: f32) -> Result<Self, String> {
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(format!(
                "temperature must be between 0.0 and 2.0, got {value}"
            ))
        }
    }

    /// Creates a new Temperature value without validation
    ///
    /// # Safety
    /// This function should only be used when the value is known to be valid
    pub fn new_unchecked(value: f32) -> Self {
        debug_assert!(Self::is_valid(value), "invalid temperature: {value}");
        Self(value)
    }

    /// Returns true if the temperature value is within the valid range (0.0 to
    /// 2.0)
    pub fn is_valid(value: f32) -> bool {
        (0.0..=2.0).contains(&value)
    }

    /// Returns the inner f32 value
    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Deref for Temperature {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Temperature> for f32 {
    fn from(temp: Temperature) -> Self {
        temp.0
    }
}

impl fmt::Display for Temperature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Temperature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert to string with fixed precision to avoid floating point issues
        // and then parse back to ensure consistent serialization
        let formatted = format!("{:.1}", self.0);
        let value = formatted.parse::<f32>().unwrap();
        serializer.serialize_f32(value)
    }
}

impl<'de> Deserialize<'de> for Temperature {
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
                "temperature must be between 0.0 and 2.0, got {value}"
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
    fn test_temperature_creation() {
        // Valid temperatures should be created successfully
        let valid_temps = [0.0, 0.5, 1.0, 1.5, 2.0];
        for temp in valid_temps {
            let result = Temperature::new(temp);
            assert!(result.is_ok(), "Temperature {temp} should be valid");
            assert_eq!(result.unwrap().value(), temp);
        }

        // Invalid temperatures should return an error
        let invalid_temps = [-0.1, 2.1, 3.0, -1.0, 10.0];
        for temp in invalid_temps {
            let result = Temperature::new(temp);
            assert!(result.is_err(), "Temperature {temp} should be invalid");
            assert!(
                result
                    .unwrap_err()
                    .contains("temperature must be between 0.0 and 2.0"),
                "Error should mention valid range"
            );
        }
    }

    #[test]
    fn test_temperature_serialization() {
        let temp = Temperature::new(0.7).unwrap();
        let json = serde_json::to_value(temp).unwrap();

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
    fn test_temperature_deserialization() {
        // Valid temperature values should deserialize correctly
        let valid_temps = [0.0, 0.5, 1.0, 1.5, 2.0];
        for temp_value in valid_temps {
            let json = json!(temp_value);
            let temp: Result<Temperature, _> = serde_json::from_value(json);
            assert!(
                temp.is_ok(),
                "Valid temperature {temp_value} should deserialize"
            );
            assert_eq!(temp.unwrap().value(), temp_value);
        }

        // Invalid temperature values should fail deserialization
        let invalid_temps = [-0.1, 2.1, 3.0, -1.0, 10.0];
        for temp_value in invalid_temps {
            let json = json!(temp_value);
            let temp: Result<Temperature, _> = serde_json::from_value(json);
            assert!(
                temp.is_err(),
                "Invalid temperature {temp_value} should fail deserialization"
            );
            let err = temp.unwrap_err().to_string();
            assert!(
                err.contains("temperature must be between 0.0 and 2.0"),
                "Error should mention valid range: {err}"
            );
        }
    }

    #[test]
    fn test_temperature_in_struct() {
        #[derive(Serialize, Deserialize, Debug)]
        struct TestStruct {
            temp: Temperature,
        }

        // Valid temperature
        let json = json!({
            "temp": 0.7
        });
        let test_struct: Result<TestStruct, _> = serde_json::from_value(json);
        assert!(test_struct.is_ok());
        assert_eq!(test_struct.unwrap().temp.value(), 0.7);

        // Invalid temperature
        let json = json!({
            "temp": 2.5
        });
        let test_struct: Result<TestStruct, _> = serde_json::from_value(json);
        assert!(test_struct.is_err());
        let err = test_struct.unwrap_err().to_string();
        assert!(
            err.contains("temperature must be between 0.0 and 2.0"),
            "Error should mention valid range: {err}"
        );
    }
}
