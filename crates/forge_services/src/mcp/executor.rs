use std::borrow::Cow;
use std::sync::Arc;

use forge_domain::{ExecutableTool, ToolCallContext, ToolDefinition, ToolName};
use rmcp::model::CallToolRequestParam;
use rmcp::schemars::schema::RootSchema;

use crate::mcp::service::RunnableService;

pub struct McpExecutor {
    pub client: Arc<RunnableService>,
    pub tool_definition: ToolDefinition,
}

impl McpExecutor {
    pub fn new(
        server_name: String,
        tool: rmcp::model::Tool,
        client: Arc<RunnableService>,
    ) -> anyhow::Result<Self> {
        let name = ToolName::new(tool.name).server(server_name);
        let input_schema: RootSchema = serde_json::from_value(serde_json::Value::Object(
            tool.input_schema.as_ref().clone(),
        ))?;

        Ok(Self {
            client,
            tool_definition: ToolDefinition::new(name.to_string())
                .description(tool.description.unwrap_or_default().to_string())
                .input_schema(input_schema),
        })
    }
}

#[async_trait::async_trait]
impl ExecutableTool for McpExecutor {
    type Input = serde_json::Value;

    async fn call(&self, _: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
        let result = self
            .client
            .call_tool(CallToolRequestParam {
                name: Cow::Owned(self.tool_definition.name.name.to_string()),
                arguments: if let serde_json::Value::Object(args) = input {
                    Some(args)
                } else {
                    None
                },
            })
            .await?;

        let content = serde_json::to_string(&result.content)?;

        if result.is_error.unwrap_or_default() {
            anyhow::bail!("{}", content)
        } else {
            Ok(content)
        }
    }
}
