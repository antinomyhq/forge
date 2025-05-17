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
pub enum ToolContentItem {
    Text(String),
    Base64URL(String),
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct ToolContent {
    pub items: Vec<ToolContentItem>,
    pub is_error: bool,
}

impl ToolContent {
    pub fn text(tool: String) -> Self {
        ToolContent { is_error: false, items: vec![ToolContentItem::Text(tool)] }
    }

    pub fn image(url: String) -> Self {
        ToolContent {
            is_error: false,
            items: vec![ToolContentItem::Base64URL(url)],
        }
    }

    pub fn into_string(self) -> String {
        match self {
            ToolContent { items, .. } => items
                .into_iter()
                .map(|item| match item {
                    ToolContentItem::Text(text) => text,
                    ToolContentItem::Base64URL(image) => image,
                    ToolContentItem::Audio(url) => url,
                    ToolContentItem::Resource(url) => url,
                })
                .collect(),
        }
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
