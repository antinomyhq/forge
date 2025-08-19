use std::collections::BTreeMap;

use forge_json_repair::json_repair;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Error;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum ToolCallArguments {
    Unparsed(String),
    Parsed(Value),
}

impl Default for ToolCallArguments {
    fn default() -> Self {
        ToolCallArguments::Parsed(Value::default())
    }
}

impl ToolCallArguments {
    pub fn into_string(self) -> String {
        match self {
            ToolCallArguments::Unparsed(str) => str,
            ToolCallArguments::Parsed(value) => value.to_string(),
        }
    }

    pub fn parse(&self) -> Result<Value, Error> {
        match self {
            ToolCallArguments::Unparsed(json) => {
                Ok(
                    json_repair(json).map_err(|error| crate::Error::ToolCallArgument {
                        error,
                        args: json.to_owned(),
                    })?,
                )
            }
            ToolCallArguments::Parsed(value) => Ok(value.to_owned()),
        }
    }

    pub fn from_json(str: &str) -> ToolCallArguments {
        ToolCallArguments::Unparsed(str.to_string())
    }

    pub fn from_object(object: BTreeMap<String, String>) -> ToolCallArguments {
        let mut map = Map::new();

        for (key, value) in object {
            map.insert(key, Value::from(value));
        }

        ToolCallArguments::Parsed(Value::Object(map))
    }
}

impl<'a> From<&'a str> for ToolCallArguments {
    fn from(value: &'a str) -> Self {
        ToolCallArguments::from_json(value)
    }
}

impl From<Value> for ToolCallArguments {
    fn from(value: Value) -> Self {
        ToolCallArguments::Parsed(value)
    }
}
