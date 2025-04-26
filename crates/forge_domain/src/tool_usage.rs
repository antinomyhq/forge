use std::collections::HashSet;
use std::fmt::Display;

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
        for (i, tool) in self.tools.iter().enumerate() {
            writeln!(f, "{}. {}:", i + 1, tool.name.as_str())?;
            writeln!(f, "Description: {}", &tool.description)?;
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
                    props
                        .into_object()
                        .metadata
                        .into_iter()
                        .map(move |meta| (name.clone(), meta))
                })
                .flat_map(|(name, meta)| {
                    meta.description
                        .into_iter()
                        .map(move |desc| (name.clone(), desc))
                })
                .collect::<Vec<_>>();

            writeln!(f, "Usage:")?;
            writeln!(f, "<forge_tool_call>")?;
            writeln!(f, "<{}>", tool.name.as_str())?;

            for (parameter, desc) in parameters {
                writeln!(f, "<{parameter}>")?;

                if required.contains(&parameter) {
                    writeln!(f, "<!-- required -->")?;
                }

                writeln!(f, "<!-- {desc} -->")?;

                writeln!(f, "</{parameter}>")?;
            }

            writeln!(f, "</{}>", tool.name.as_str())?;
            writeln!(f, "</forge_tool_call>\n\n")?;
        }

        Ok(())
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

        async fn call(&self, _: ToolCallContext, _: Self::Input) -> anyhow::Result<String> {
            Ok("Completed".to_string())
        }
    }

    #[test]
    fn test_tool_usage_prompt_to_string() {
        let tools = vec![ToolDefinition::from(&MangoTool)];
        let prompt = ToolUsagePrompt::from(&tools);
        assert_snapshot!(prompt);
    }
}
