use std::collections::{BTreeMap, HashSet};
use std::fmt::Display;

use serde::Serialize;

use crate::ToolDefinition;

pub struct ToolUsagePrompt<'a> {
    tools: &'a Vec<ToolDefinition>,
}

impl<'a> From<&'a Vec<ToolDefinition>> for ToolUsagePrompt<'a> {
    fn from(value: &'a Vec<ToolDefinition>) -> Self {
        Self { tools: value }
    }
}

impl Display for ToolUsagePrompt<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for tool in self.tools.iter() {
            // Get the schema as an object
            let schema_obj = match tool.input_schema.as_object() {
                Some(obj) => obj,
                None => continue, // Skip if not an object schema
            };

            // Extract required fields
            let required = schema_obj
                .get("required")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect::<HashSet<_>>()
                })
                .unwrap_or_default();

            // Extract properties
            let empty_map = serde_json::Map::new();
            let properties = schema_obj
                .get("properties")
                .and_then(|v| v.as_object())
                .unwrap_or(&empty_map);

            let parameters = properties
                .iter()
                .filter_map(|(name, prop_schema)| {
                    let prop_obj = prop_schema.as_object()?;

                    // Extract description
                    let description = prop_obj
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    // Extract type information
                    let type_of = prop_obj.get("type").cloned();

                    let parameter =
                        Parameter { description, type_of, is_required: required.contains(name) };

                    Some((name.clone(), parameter))
                })
                .collect::<BTreeMap<_, _>>();

            let schema = Schema {
                name: tool.name.to_string(),
                arguments: parameters,
                description: tool.description.clone(),
            };

            writeln!(f, "<tool>{schema}</tool>")?;
        }

        Ok(())
    }
}

#[derive(Serialize)]
struct Schema {
    name: String,
    description: String,
    arguments: BTreeMap<String, Parameter>,
}

#[derive(Serialize)]
struct Parameter {
    description: String,
    #[serde(rename = "type")]
    type_of: Option<serde_json::Value>,
    is_required: bool,
}

impl Display for Schema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}

#[cfg(test)]
mod tests {

    use insta::assert_snapshot;
    use schemars::JsonSchema;
    use serde::Deserialize;

    use super::*;
    use crate::{
        ExecutableTool, NamedTool, ToolCallContext, ToolDefinition, ToolDescription, ToolName,
        ToolOutput,
    };

    #[derive(Default)]
    pub struct MangoTool;

    #[derive(JsonSchema, Deserialize)]
    pub struct ToolInput {
        /// This is parameter 1
        #[allow(dead_code)]
        param1: String,

        /// This is parameter 2
        #[allow(dead_code)]
        param2: Option<String>,
    }

    impl ToolDescription for MangoTool {
        fn description(&self) -> String {
            "This is a mango tool".to_string()
        }
    }

    impl NamedTool for MangoTool {
        fn tool_name() -> ToolName {
            ToolName::new("forge_tool_mango")
        }
    }

    #[async_trait::async_trait]
    impl ExecutableTool for MangoTool {
        type Input = ToolInput;

        async fn call(&self, _: ToolCallContext, _: Self::Input) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::text("Completed".to_string()))
        }
    }

    #[test]
    fn test_tool_usage_prompt_to_string() {
        let tools = vec![ToolDefinition::from(&MangoTool)];
        let prompt = ToolUsagePrompt::from(&tools);
        assert_snapshot!(prompt);
    }
}
