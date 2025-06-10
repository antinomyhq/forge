use schemars::JsonSchema;
use serde_json::Value;

use crate::{
    ExecutableTool, NamedTool, ToolCallContext, ToolDefinition, ToolDescription, ToolOutput,
};

struct JsonTool<T>(T);

impl<T> JsonTool<T> {
    pub fn new(tool: T) -> Self {
        Self(tool)
    }
}

#[async_trait::async_trait]
impl<T: ExecutableTool + Sync> ExecutableTool for JsonTool<T>
where
    T::Input: serde::de::DeserializeOwned + JsonSchema,
{
    type Input = Value;

    async fn call(
        &self,
        context: &mut ToolCallContext,
        input: Self::Input,
    ) -> anyhow::Result<ToolOutput> {
        let input: T::Input = serde_json::from_value(input)?;
        self.0.call(context, input).await
    }
}

pub struct Tool {
    pub executable: Box<dyn ExecutableTool<Input = Value> + Send + Sync + 'static>,
    pub definition: ToolDefinition,
}

impl<T> From<T> for Tool
where
    T: ExecutableTool + ToolDescription + NamedTool + Send + Sync + 'static,
    T::Input: serde::de::DeserializeOwned + JsonSchema,
{
    fn from(tool: T) -> Self {
        let definition = ToolDefinition::from(&tool);
        let executable = Box::new(JsonTool::new(tool));

        Tool { executable, definition }
    }
}
