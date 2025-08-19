use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ToolCallArguments {
    #[default]
    Empty,
    Json(String),
    Object(BTreeMap<String, String>),
}

impl ToolCallArguments {
    pub fn as_str(&self) -> &str {
        todo!()
    }

    pub fn parse(&self) -> anyhow::Result<Value> {
        todo!()
    }

    pub fn from_json(str: &str) -> ToolCallArguments {
        ToolCallArguments::Json(str.to_string())
    }

    pub fn from_object(object: BTreeMap<String, String>) -> ToolCallArguments {
        ToolCallArguments::Object(object)
    }
}

impl<'a> From<&'a str> for ToolCallArguments {
    fn from(value: &'a str) -> Self {
        ToolCallArguments::from_json(value)
    }
}

impl Serialize for ToolCallArguments {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        todo!()
    }
}

impl<'de> Deserialize<'de> for ToolCallArguments {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        todo!()
    }
}

impl From<Value> for ToolCallArguments {
    fn from(value: Value) -> Self {
        todo!()
    }
}
