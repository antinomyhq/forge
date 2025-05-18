use derive_setters::Setters;
use schemars::schema::RootSchema;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{NamedTool, ToolCallContext, ToolName, ToolOutput};

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

// FIXME: Drop this and use ToolResult instead
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct ToolContent {
    pub items: Vec<ToolOutput>,
    pub is_error: bool,
}

impl ToolContent {
    pub fn text(tool: String) -> Self {
        ToolContent { is_error: false, items: vec![ToolOutput::Text(tool)] }
    }

    pub fn image(url: String) -> Self {
        ToolContent { is_error: false, items: vec![ToolOutput::Base64URL(url)] }
    }

    pub fn combine(self, other: ToolContent) -> Self {
        let mut items = self.items;
        items.extend(other.items);
        ToolContent { items, is_error: self.is_error || other.is_error }
    }
}

impl<T> From<T> for ToolContent
where
    T: Iterator<Item = ToolContent>,
{
    fn from(item: T) -> Self {
        item.fold(ToolContent::default(), |acc, item| acc.combine(item))
    }
}
