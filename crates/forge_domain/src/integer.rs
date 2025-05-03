use std::fmt;
use std::ops::Deref;

use schemars::gen::SchemaGenerator;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A non-negative integer that serializes as i64 in JSON Schema for
/// compatibility with providers like Google AI Studio that don't support 'uint'
/// format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NonNegativeInteger(usize);

impl NonNegativeInteger {
    pub fn new(value: usize) -> Self {
        Self(value)
    }

    pub fn value(&self) -> usize {
        self.0
    }
}

impl Deref for NonNegativeInteger {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// Custom serialization
impl Serialize for NonNegativeInteger {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0 as u64)
    }
}

// Custom deserialization
impl<'de> Deserialize<'de> for NonNegativeInteger {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Ok(NonNegativeInteger(value as usize))
    }
}

// This is where the magic happens - customizing the JSON Schema representation
impl JsonSchema for NonNegativeInteger {
    fn schema_name() -> String {
        "NonNegativeInteger".to_string()
    }

    fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
        let mut schema = SchemaObject {
            instance_type: Some(InstanceType::Integer.into()),
            format: Some("int64".to_string()), /* Using int64 format for Google AI Studio
                                                * compatibility */
            ..Default::default()
        };

        // Add minimum constraint for non-negative values
        schema.number = Some(Box::new(schemars::schema::NumberValidation {
            minimum: Some(0.0),
            ..Default::default()
        }));

        // Add description
        schema.metadata().description = Some("A non-negative integer value".to_string());

        schema.into()
    }
}

impl fmt::Display for NonNegativeInteger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Add convenience for Option<NonNegativeInteger>
impl From<usize> for NonNegativeInteger {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
