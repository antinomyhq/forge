use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    None,
    Auto,
    Required,
    #[serde(untagged)]
    Function {
        r#type: FunctionType,
        function: FunctionName,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct FunctionName {
    pub name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct FunctionType;

impl Serialize for FunctionType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("function")
    }
}

impl<'de> Deserialize<'de> for FunctionType {
    fn deserialize<D>(_deserializer: D) -> Result<FunctionType, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(FunctionType)
    }
}

impl From<forge_domain::ToolChoice> for ToolChoice {
    fn from(value: forge_domain::ToolChoice) -> Self {
        match value {
            forge_domain::ToolChoice::None => ToolChoice::None,
            forge_domain::ToolChoice::Auto => ToolChoice::Auto,
            forge_domain::ToolChoice::Required => ToolChoice::Required,
            forge_domain::ToolChoice::Call(tool_name) => ToolChoice::Function {
                function: FunctionName { name: tool_name.into_string() },
                r#type: FunctionType,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_choice_serialization() {
        // Test None variant
        let choice_none = ToolChoice::None;
        assert_eq!(serde_json::to_string(&choice_none).unwrap(), r#""none""#);

        // Test Auto variant
        let choice_auto = ToolChoice::Auto;
        assert_eq!(serde_json::to_string(&choice_auto).unwrap(), r#""auto""#);

        // Test Function variant
        let choice_function = ToolChoice::Function {
            function: FunctionName { name: "test_tool".to_string() },
            r#type: FunctionType,
        };
        assert_eq!(
            serde_json::to_string(&choice_function).unwrap(),
            r#"{"type":"function","function":{"name":"test_tool"}}"#
        );
    }
}
