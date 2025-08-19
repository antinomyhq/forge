use derive_more::From;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, From, PartialEq, Eq)]
pub struct ToolCallArguments(String);

impl From<&str> for ToolCallArguments {
    fn from(value: &str) -> Self {
        todo!()
    }
}

impl ToolCallArguments {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse(&self) -> anyhow::Result<Value> {
        Ok(serde_json::from_str(&self.0)?)
    }
}

impl Serialize for ToolCallArguments {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Parse the JSON string and serialize the resulting Value
        let value: Value = serde_json::from_str(&self.0).map_err(serde::ser::Error::custom)?;
        value.serialize(_serializer)
    }
}

impl<'de> Deserialize<'de> for ToolCallArguments {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize as a Value first, then convert to JSON string
        let value = Value::deserialize(_deserializer)?;
        let json_string = serde_json::to_string(&value).map_err(serde::de::Error::custom)?;
        Ok(ToolCallArguments(json_string))
    }
}

impl From<Value> for ToolCallArguments {
    fn from(value: Value) -> Self {
        todo!()
    }
}
