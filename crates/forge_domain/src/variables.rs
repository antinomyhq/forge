use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

#[derive(Default, Serialize)]
pub struct Variables(HashMap<String, Value>);

impl Variables {
    pub fn add(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.0.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn merge(self, other: Self) -> Self {
        let mut merged = self;
        merged.0.extend(other.0);
        merged
    }

    pub fn default_key() -> &'static str {
        "value"
    }
}

impl From<Vec<Variables>> for Variables {
    fn from(value: Vec<Variables>) -> Self {
        value
            .into_iter()
            .reduce(|a, b| a.merge(b))
            .unwrap_or_default()
    }
}

impl From<Value> for Variables {
    fn from(value: Value) -> Self {
        let mut variables = Variables::default();
        match value {
            Value::Null => {}
            Value::Bool(value) => {
                variables.add(Self::default_key(), value.to_string());
            }
            Value::Number(value) => {
                variables.add(Self::default_key(), value.to_string());
            }
            Value::String(value) => {
                variables.add(Self::default_key(), value);
            }
            Value::Array(values) => {
                variables.add(Self::default_key(), values);
            }
            Value::Object(map) => {
                for (key, value) in map {
                    variables.add(key, value);
                }
            }
        };

        variables
    }
}
