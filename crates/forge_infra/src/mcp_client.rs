use std::borrow::Cow;

use forge_domain::{ToolDefinition, ToolName};
use forge_services::McpClient;
use rmcp::model::{CallToolRequestParam, InitializeRequestParam};
use rmcp::schemars::schema::RootSchema;
use rmcp::service::RunningService;
use rmcp::RoleClient;
use serde_json::Value;

pub struct ForgeMcpClient {
    client: RunningService<RoleClient, InitializeRequestParam>,
}

impl ForgeMcpClient {
    pub fn new(client: RunningService<RoleClient, InitializeRequestParam>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl McpClient for ForgeMcpClient {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let tools = self.client.list_tools(None).await?;
        Ok(tools
            .tools
            .into_iter()
            .filter_map(|tool| {
                Some(
                    ToolDefinition::new(tool.name)
                        .description(tool.description.unwrap_or_default())
                        .input_schema(
                            serde_json::from_value::<RootSchema>(Value::Object(
                                tool.input_schema.as_ref().clone(),
                            ))
                            .ok()?,
                        ),
                )
            })
            .collect())
    }

    async fn call_tool(&self, tool_name: &ToolName, input: Value) -> anyhow::Result<String> {
        let result = self
            .client
            .call_tool(CallToolRequestParam {
                name: Cow::Owned(tool_name.to_string()),
                arguments: if let Value::Object(args) = input {
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
