use std::collections::HashMap;
use std::sync::Arc;

use forge_domain::{
    Tool, ToolCallContext, ToolCallFull, ToolDefinition, ToolName, ToolResult, ToolService,
};
use tokio::time::{timeout, Duration};
use tracing::{debug, error};

use crate::tools::ToolRegistry;
use crate::Infrastructure;

// Timeout duration for tool calls
const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct ForgeToolService<M> {
    tools: Arc<HashMap<ToolName, Tool>>,
    mcp: Arc<M>,
}

impl<M> ForgeToolService<M> {
    pub fn new<F: Infrastructure>(infra: Arc<F>, mcp: Arc<M>) -> Self {
        let registry = ToolRegistry::new(infra.clone());
        let tools: HashMap<ToolName, Tool> = registry
            .tools()
            .into_iter()
            .map(|tool| (tool.definition.name.clone(), tool))
            .collect::<HashMap<_, _>>();

        Self { tools: Arc::new(tools), mcp }
    }
}
impl<M: ToolService> ForgeToolService<M> {
    async fn call(&self, context: ToolCallContext, call: ToolCallFull) -> ToolResult {
        let name = call.name.clone();
        let input = call.arguments.clone();
        debug!(tool_name = ?call.name, arguments = ?call.arguments, "Executing tool call");

        let output = match self.tools.get(&name) {
            Some(tool) => {
                // Wrap tool call with timeout
                match timeout(TOOL_CALL_TIMEOUT, tool.executable.call(context, input)).await {
                    Ok(result) => result,
                    Err(_) => Err(anyhow::anyhow!(
                        "Tool '{}' timed out after {} minutes",
                        name.as_str(),
                        TOOL_CALL_TIMEOUT.as_secs() / 60
                    )),
                }
            }
            None => {
                let mut available_tools = self
                    .tools
                    .keys()
                    .map(|name| name.as_str())
                    .collect::<Vec<_>>();

                available_tools.sort();

                Err(anyhow::anyhow!(
                    "No tool with name '{}' was found. Please try again with one of these tools {}",
                    name.as_str(),
                    available_tools.join(", ")
                ))
            }
        };

        let result = match output {
            Ok(output) => ToolResult::from(call).success(output),
            Err(output) => {
                error!(error = ?output, "Tool call failed");
                ToolResult::from(call).failure(output)
            }
        };

        debug!(result = ?result, "Tool call result");
        result
    }

    fn list(&self) -> Vec<ToolDefinition> {
        let mut tools: Vec<_> = self
            .tools
            .values()
            .map(|tool| tool.definition.clone())
            .collect();

        // Add all MCP tools
        tools.extend(self.mcp.list());

        // Sorting is required to ensure system prompts are exactly the same
        tools.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        tools
    }

    fn find(&self, name: &ToolName) -> Option<Tool> {
        let name = name.clone();

        self.tools
            .get(&name)
            .or(self.mcp.find(&name).as_ref())
            .cloned()
    }
}

#[async_trait::async_trait]
impl<M: ToolService> ToolService for ForgeToolService<M> {
    async fn call(&self, context: ToolCallContext, call: ToolCallFull) -> ToolResult {
        self.call(context, call).await
    }

    fn list(&self) -> Vec<ToolDefinition> {
        self.list()
    }

    fn find(&self, name: &ToolName) -> Option<Tool> {
        self.find(name)
    }
}

#[cfg(test)]
mod test {
    use anyhow::bail;
    use forge_domain::{Tool, ToolCallContext, ToolCallId, ToolDefinition};
    use serde_json::{json, Value};
    use tokio::time;

    use super::*;

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
            executable: Arc::new(SuccessTool),
        };

        let failure_tool = Tool {
            definition: ToolDefinition {
                name: ToolName::new("failure_tool"),
                description: "A test tool that always fails".to_string(),
                input_schema: schemars::schema_for!(serde_json::Value),
                output_schema: Some(schemars::schema_for!(String)),
            },
            executable: Arc::new(FailureTool),
        };

        ForgeToolService::from_iter(vec![success_tool, failure_tool])
    }

    struct Stub;

    #[async_trait::async_trait]
    impl ToolService for Stub {
        async fn call(&self, _: ToolCallContext, _: ToolCallFull) -> ToolResult {
            unimplemented!()
        }
        fn list(&self) -> Vec<ToolDefinition> {
            Default::default()
        }
        fn find(&self, _: &ToolName) -> Option<Tool> {
            None
        }
    }

    impl ForgeToolService<Stub> {
        fn from_iter(tools: Vec<Tool>) -> Self {
            ForgeToolService {
                tools: Arc::new(HashMap::from_iter(
                    tools
                        .into_iter()
                        .map(|tool| (tool.definition.name.clone(), tool)),
                )),
                mcp: Arc::new(Stub),
            }
        }
    }

    #[tokio::test]
    async fn test_successful_tool_call() {
        let service = new_tool_service();
        let call = ToolCallFull {
            name: ToolName::new("success_tool"),
            arguments: json!("test input"),
            call_id: Some(ToolCallId::new("test")),
        };

        let result = service.call(ToolCallContext::default(), call).await;
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

        let result = service.call(ToolCallContext::default(), call).await;
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

        let result = service.call(ToolCallContext::default(), call).await;
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
            executable: Arc::new(SlowTool),
        };

        let service = ForgeToolService::from_iter(vec![slow_tool]);
        let call = ToolCallFull {
            name: ToolName::new("slow_tool"),
            arguments: json!("test input"),
            call_id: Some(ToolCallId::new("test")),
        };

        // Advance time to trigger timeout
        test::time::advance(Duration::from_secs(305)).await;

        let result = service.call(ToolCallContext::default(), call).await;

        // Assert that the result contains a timeout error message
        let content_str = &result.content;
        assert!(
            content_str.contains("timed out"),
            "Expected timeout error message"
        );
        assert!(result.is_error, "Expected error result for timeout");
    }
}
