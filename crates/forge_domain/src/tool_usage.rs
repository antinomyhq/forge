use std::collections::{BTreeMap, HashSet};
use std::fmt::Display;

use schemars::schema::{InstanceType, SingleOrVec};
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
            let required = tool
                .input_schema
                .schema
                .clone()
                .object
                .iter()
                .flat_map(|object| object.required.clone().into_iter())
                .collect::<HashSet<_>>();

            let parameters = tool
                .input_schema
                .schema
                .object
                .clone()
                .into_iter()
                .flat_map(|object| object.properties.into_iter())
                .flat_map(|(name, props)| {
                    let object = props.into_object();
                    let instance = object.instance_type.clone();
                    object
                        .metadata
                        .into_iter()
                        .map(move |meta| (name.clone(), meta, instance.clone()))
                })
                .flat_map(|(name, meta, instance)| {
                    meta.description
                        .into_iter()
                        .map(move |desc| (name.clone(), desc, instance.clone()))
                })
                .map(|(name, desc, instance)| {
                    let parameter = Parameter {
                        description: desc,
                        type_of: instance,
                        is_required: required.contains(&name),
                    };

                    (name, parameter)
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
    type_of: Option<SingleOrVec<InstanceType>>,
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
    use strum::IntoEnumIterator;

    use super::*;
    use crate::{NamedTool, ToolDescription, ToolName, Tools};

    #[derive(Default)]
    #[allow(dead_code)]
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

    #[test]
    fn test_tool_usage() {
        let tools = Tools::iter().map(|v| v.definition()).collect::<Vec<_>>();
        let prompt = ToolUsagePrompt::from(&tools);
        assert_snapshot!(prompt);
    }
}
