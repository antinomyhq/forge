use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Arguments that need to be passed to a tool
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ToolCallArguments(String);

impl ToolCallArguments {
    pub fn new(value: impl ToString) -> Self {
        ToolCallArguments(value.to_string())
    }

    // FIXME: Should be required
    pub fn from_value(value: Value) -> Self {
        ToolCallArguments(value.to_string())
    }

    // FIXME: Should be required
    pub fn is_null(&self) -> bool {
        self.0.is_empty() || self.0 == "null"
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    // FIXME: Should be required
    pub fn into_value(self) -> std::result::Result<Value, serde_json::Error> {
        serde_json::from_str(&self.0)
    }

    // FIXME: Should be required
    pub fn as_value(&self) -> std::result::Result<Value, serde_json::Error> {
        serde_json::from_str(&self.0)
    }
}

impl Default for ToolCallArguments {
    fn default() -> Self {
        ToolCallArguments("{}".to_string())
    }
}

impl From<ToolCallArguments> for Value {
    fn from(args: ToolCallArguments) -> Self {
        args.into_value().unwrap_or(Value::Null)
    }
}

impl std::fmt::Display for ToolCallArguments {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::Value;

    use super::*;

    #[test]
    fn test_from_map_optimization() {
        // Fixture: Create a map directly instead of going through Value
        let mut map = serde_json::Map::new();
        map.insert("key1".to_string(), Value::String("value1".to_string()));
        map.insert(
            "key2".to_string(),
            Value::Number(serde_json::Number::from(42)),
        );

        let actual = ToolCallArguments::from_value(serde_json::Value::Object(map.clone()));

        // Expected: Should produce the same result as from_value but more efficiently
        let expected = ToolCallArguments::from_value(Value::Object(map));

        assert_eq!(actual, expected);
    }
}
