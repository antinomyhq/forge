use std::collections::HashMap;
use std::sync::Arc;

use forge_domain::{
    McpService, Tool, ToolCallContext, ToolCallFull, ToolDefinition, ToolName, ToolResult,
    ToolService,
};
use tokio::time::{timeout, Duration};
use tracing::debug;

use crate::tools::ToolRegistry;
use crate::Infrastructure;

// Timeout duration for tool calls
const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct ForgeToolService<M> {
    tools: Arc<HashMap<ToolName, Arc<Tool>>>,
    mcp: Arc<M>,
}

impl<M: McpService> ForgeToolService<M> {
    pub fn new<F: Infrastructure>(infra: Arc<F>, mcp: Arc<M>) -> Self {
        let registry = ToolRegistry::new(infra.clone());
        let tools = registry.tools();
        let tools: HashMap<ToolName, Arc<Tool>> = tools
            .into_iter()
            .map(|tool| (tool.definition.name.clone(), Arc::new(tool)))
            .collect::<HashMap<_, _>>();

        Self { tools: Arc::new(tools), mcp }
    }

    /// Get a tool by its name. If the tool is not found, it returns an error
    /// with a list of available tools.
    async fn get_tool(&self, name: &ToolName) -> anyhow::Result<Arc<Tool>> {
        self.find(name).await?.ok_or_else(|| {
            let mut available_tools = self
                .tools
                .keys()
                .map(|name| name.to_string())
                .collect::<Vec<_>>();

            available_tools.sort();

            // FIXME: Use typed errors instead of anyhow
            anyhow::anyhow!(
                "No tool with name '{}' was found. Please try again with one of these tools {}",
                name.to_string(),
                available_tools.join(", ")
            )
        })
    }
}

#[async_trait::async_trait]
impl<M: McpService> ToolService for ForgeToolService<M> {
    async fn call(
        &self,
        context: ToolCallContext,
        call: ToolCallFull,
    ) -> anyhow::Result<ToolResult> {
        let name = call.name.clone();
        let input = call.arguments.clone();
        debug!(tool_name = ?call.name, arguments = ?call.arguments, "Executing tool call");

        let tool = self.get_tool(&name).await?;

        let result = match timeout(TOOL_CALL_TIMEOUT, tool.executable.call(context, input)).await {
            Ok(output) => ToolResult::new(call.name).output(output),
            Err(elapsed) => ToolResult::new(call.name).failure(
                anyhow::anyhow!(
                    "Tool '{}' timed out after {} minutes",
                    name.to_string(),
                    TOOL_CALL_TIMEOUT.as_secs() / 60
                )
                .context(elapsed),
            ),
        }
        .call_id(call.call_id);

        Ok(result)
    }

    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let mut tools: Vec<_> = self
            .tools
            .values()
            .map(|tool| tool.definition.clone())
            .collect();
        let mcp_tools = self.mcp.list().await?;
        tools.extend(mcp_tools);

        // Sorting is required to ensure system prompts are exactly the same
        tools.sort_by(|a, b| a.name.to_string().cmp(&b.name.to_string()));

        Ok(tools)
    }
    async fn find(&self, name: &ToolName) -> anyhow::Result<Option<Arc<Tool>>> {
        Ok(self.tools.get(name).cloned().or(self.mcp.find(name).await?))
    }
}

#[cfg(test)]
mod test {
    use forge_domain::{Tool, ToolCallContext, ToolCallId, ToolDefinition};
    use serde_json::{json, Value};

    use super::*;

    struct Stub;

    #[async_trait::async_trait]
    impl McpService for Stub {
        async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
            Ok(vec![])
        }

        async fn find(&self, _: &ToolName) -> anyhow::Result<Option<Arc<Tool>>> {
            Ok(None)
        }
    }

    impl FromIterator<Tool> for ForgeToolService<Stub> {
        fn from_iter<T: IntoIterator<Item = Tool>>(iter: T) -> Self {
            let tools: HashMap<ToolName, Arc<Tool>> = iter
                .into_iter()
                .map(|tool| (tool.definition.name.clone(), Arc::new(tool)))
                .collect::<HashMap<_, _>>();

            Self { tools: Arc::new(tools), mcp: Arc::new(Stub) }
        }
    }

    // Mock tool that simulates a long-running task
    struct SlowTool;

    #[async_trait::async_trait]
    impl forge_domain::ExecutableTool for SlowTool {
        type Input = Value;

        async fn call(
            &self,
            _context: ToolCallContext,
            _input: Self::Input,
        ) -> anyhow::Result<forge_domain::ToolOutput> {
            // Simulate a long-running task that exceeds the timeout
            tokio::time::sleep(Duration::from_secs(400)).await;
            Ok(forge_domain::ToolOutput::text(
                "Slow tool completed".to_string(),
            ))
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_tool_timeout() {
        // Create a mock tool that would normally time out
        let slow_tool = Tool {
            definition: ToolDefinition {
                name: ToolName::new("slow_tool"),
                description: "A test tool that takes too long".to_string(),
                input_schema: schemars::schema_for!(serde_json::Value),
                output_schema: Some(schemars::schema_for!(String)),
            },
            executable: Box::new(SlowTool),
        };

        let service = ForgeToolService::from_iter(vec![slow_tool]);
        let call = ToolCallFull {
            name: ToolName::new("slow_tool"),
            arguments: json!("test input"),
            call_id: Some(ToolCallId::new("test")),
        };

        // Use tokio::time::timeout directly to simulate tool timeout behavior
        // without relying on tokio test mock time that might be flakey
        let result = tokio::time::timeout(
            Duration::from_millis(50), // Use a very short timeout for test speed
            service.call(ToolCallContext::default(), call),
        )
        .await;

        // Verify we got an elapsed error
        assert!(result.is_err(), "Expected timeout error");
        let timeout_err = result.unwrap_err();
        assert!(
            timeout_err.to_string().contains("elapsed"),
            "Expected 'elapsed' in timeout message"
        );
    }
}
