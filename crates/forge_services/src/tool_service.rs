use std::collections::HashMap;
use std::sync::Arc;

use forge_domain::{
    McpService, Tool, ToolCallContext, ToolCallFull, ToolDefinition, ToolName, ToolResult,
    ToolService,
};
use tokio::time::{timeout, Duration};
use tracing::{debug, error};

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

        let mut available_tools = self
            .tools
            .keys()
            .map(|name| name.to_string())
            .collect::<Vec<_>>();

        available_tools.sort();

        let output = match self.find(&name).await? {
            Some(tool) => {
                // Wrap tool call with timeout
                match timeout(TOOL_CALL_TIMEOUT, tool.executable.call(context, input)).await {
                    Ok(result) => result,
                    Err(_) => Err(anyhow::anyhow!(
                        "Tool '{}' timed out after {} minutes",
                        name.to_string(),
                        TOOL_CALL_TIMEOUT.as_secs() / 60
                    )),
                }
            }
            None => Err(anyhow::anyhow!(
                "No tool with name '{}' was found. Please try again with one of these tools {}",
                name.to_string(),
                available_tools.join(", ")
            )),
        };

        let result = match output {
            Ok(output) => ToolResult::from(call).success(output),
            Err(output) => {
                error!(error = ?output, "Tool call failed");
                ToolResult::from(call).failure(output)
            }
        };

        debug!(result = ?result, "Tool call result");
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
    use anyhow::bail;
    use forge_domain::{Tool, ToolCallContext, ToolCallId, ToolDefinition};
    use serde_json::{json, Value};
    use tokio::time;

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

    // Mock tool that always succeeds
    struct SuccessTool;

    #[async_trait::async_trait]
    impl forge_domain::ExecutableTool for SuccessTool {
        type Input = Value;

        async fn call(
            &self,
            _context: ToolCallContext,
            input: Self::Input,
        ) -> anyhow::Result<String> {
            Ok(format!("Success with input: {input}"))
        }
    }

    // Mock tool that always fails
    struct FailureTool;

    #[async_trait::async_trait]
    impl forge_domain::ExecutableTool for FailureTool {
        type Input = Value;

        async fn call(
            &self,
            _context: ToolCallContext,
            _input: Self::Input,
        ) -> anyhow::Result<String> {
            bail!("Tool call failed with simulated failure".to_string())
        }
    }

    fn new_tool_service() -> impl ToolService {
        let success_tool = Tool {
            definition: ToolDefinition {
                name: ToolName::new("success_tool"),
                description: "A test tool that always succeeds".to_string(),
                input_schema: schemars::schema_for!(serde_json::Value),
                output_schema: Some(schemars::schema_for!(String)),
            },
            executable: Box::new(SuccessTool),
        };

        let failure_tool = Tool {
            definition: ToolDefinition {
                name: ToolName::new("failure_tool"),
                description: "A test tool that always fails".to_string(),
                input_schema: schemars::schema_for!(serde_json::Value),
                output_schema: Some(schemars::schema_for!(String)),
            },
            executable: Box::new(FailureTool),
        };

        ForgeToolService::from_iter(vec![success_tool, failure_tool])
    }

    #[tokio::test]
    async fn test_successful_tool_call() {
        let service = new_tool_service();
        let call = ToolCallFull {
            name: ToolName::new("success_tool"),
            arguments: json!("test input"),
            call_id: Some(ToolCallId::new("test")),
        };

        let result = service
            .call(ToolCallContext::default(), call)
            .await
            .unwrap();
        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_failed_tool_call() {
        let service = new_tool_service();
        let call = ToolCallFull {
            name: ToolName::new("failure_tool"),
            arguments: json!("test input"),
            call_id: Some(ToolCallId::new("test")),
        };

        let result = service
            .call(ToolCallContext::default(), call)
            .await
            .unwrap();
        insta::assert_snapshot!(result);
    }

    #[tokio::test]
    async fn test_tool_not_found() {
        let service = new_tool_service();
        let call = ToolCallFull {
            name: ToolName::new("nonexistent_tool"),
            arguments: json!("test input"),
            call_id: Some(ToolCallId::new("test")),
        };

        let result = service
            .call(ToolCallContext::default(), call)
            .await
            .unwrap();
        insta::assert_snapshot!(result);
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
        ) -> anyhow::Result<String> {
            // Simulate a long-running task that exceeds the timeout
            tokio::time::sleep(Duration::from_secs(400)).await;
            Ok("Slow tool completed".to_string())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_tool_timeout() {
        test::time::pause();

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

        // Advance time to trigger timeout
        test::time::advance(Duration::from_secs(305)).await;

        let result = service
            .call(ToolCallContext::default(), call)
            .await
            .unwrap();

        // Assert that the result contains a timeout error message
        let content_str = &result.content;
        assert!(
            content_str.contains("timed out"),
            "Expected timeout error message"
        );
        assert!(result.is_error, "Expected error result for timeout");
    }
}
