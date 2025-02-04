use std::collections::BTreeSet;

use derive_setters::Setters;
use schemars::schema::RootSchema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ExecutableTool, NamedTool, ToolName, UsageParameterPrompt, UsagePrompt};

///
/// Refer to the specification over here:
/// https://glama.ai/blog/2024-11-25-model-context-protocol-quickstart#server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct ToolDefinition {
    pub name: ToolName,
    pub description: String,
    pub input_schema: RootSchema,
    pub output_schema: Option<RootSchema>,
}

impl ToolDefinition {
    /// Create a new ToolDefinition
    pub fn new<N: ToString>(name: N) -> Self {
        ToolDefinition {
            name: ToolName::new(name),
            description: String::new(),
            input_schema: schemars::schema_for!(()), // Empty input schema
            output_schema: None,
        }
    }

    /// Usage prompt method (existing implementation)
    pub fn usage_prompt(&self) -> UsagePrompt {
        let input_parameters = self
            .input_schema
            .schema
            .object
            .clone()
            .map(|object| {
                object
                    .properties
                    .keys()
                    .map(|name| UsageParameterPrompt {
                        parameter_name: name.to_string(),
                        parameter_type: "...".to_string(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        UsagePrompt {
            tool_name: self.name.clone().into_string(),
            input_parameters,
            description: self.description.to_string(),
        }
    }
}

impl<T> From<&T> for ToolDefinition
where
    T: NamedTool + ExecutableTool + ToolDescription + Send + Sync + 'static,
    T::Input: serde::de::DeserializeOwned + JsonSchema,
{
    fn from(t: &T) -> Self {
        let input: RootSchema = schemars::schema_for!(T::Input);
        let output: RootSchema = schemars::schema_for!(String);
        let mut full_description = t.description();

        full_description.push_str("\n\nParameters:");

        let required = input
            .schema
            .clone()
            .object
            .iter()
            .flat_map(|object| object.required.clone().into_iter())
            .collect::<BTreeSet<_>>();

        for (name, desc) in input
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
        {
            full_description.push_str("\n- ");
            full_description.push_str(&name);

            if required.contains(&name) {
                full_description.push_str(" (required)");
            }

            full_description.push_str(": ");
            full_description.push_str(&desc);
        }

        ToolDefinition {
            name: T::tool_name(),
            description: full_description,
            input_schema: input,
            output_schema: Some(output),
        }
    }
}

pub trait ToolDescription {
    fn description(&self) -> String;
}
