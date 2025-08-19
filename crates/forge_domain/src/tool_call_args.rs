use derive_more::From;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, From, PartialEq, Eq)]
pub enum ToolCallArguments {
    Value(Value),
    String(String),
    #[default]
    Empty,
}

impl ToolCallArguments {
    pub fn as_str(&self) -> &str {
        todo!()
    }

    pub fn parse(&self) -> anyhow::Result<Value> {
        todo!()
    }
}

impl Serialize for ToolCallArguments {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        todo!()
    }
}

impl<'de> Deserialize<'de> for ToolCallArguments {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        todo!()
    }
}
