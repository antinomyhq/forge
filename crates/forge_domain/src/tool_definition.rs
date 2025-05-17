use derive_setters::Setters;
use schemars::schema::RootSchema;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{NamedTool, ToolCallContext, ToolName};

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
}

impl<T> From<&T> for ToolDefinition
where
    T: NamedTool + ExecutableTool + ToolDescription + Send + Sync + 'static,
    T::Input: serde::de::DeserializeOwned + JsonSchema,
{
    fn from(t: &T) -> Self {
        let input: RootSchema = schemars::schema_for!(T::Input);
        let output: RootSchema = schemars::schema_for!(String);

        ToolDefinition {
            name: T::tool_name(),
            description: t.description(),
            input_schema: input,
            output_schema: Some(output),
        }
    }
}

pub trait ToolDescription {
    fn description(&self) -> String;
}

#[async_trait::async_trait]
pub trait ExecutableTool {
    type Input: DeserializeOwned;

    async fn call(
        &self,
        context: ToolCallContext,
        input: Self::Input,
    ) -> anyhow::Result<ToolContent>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolContent {
    Text(String),
    ImageUrl(String),
}
impl ToolContent {
    pub fn text(tool: String) -> Self {
        ToolContent::Text(tool)
    }

    pub fn into_string(self) -> String {
        match self {
            ToolContent::Text(text) => text,
            ToolContent::ImageUrl(image) => image,
        }
    }

    pub fn contains(&self, needle: &str) -> bool {
        match self {
            ToolContent::Text(text) => text.contains(needle),
            ToolContent::ImageUrl(image) => image.contains(needle),
        }
    }
}
