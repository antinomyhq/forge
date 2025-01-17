use std::collections::HashMap;

use forge_domain::{Tool, ToolCallFull, ToolDefinition, ToolName, ToolResult};
use tokio::time::{timeout, Duration};
use tracing::debug;

use super::Service;

// Timeout duration for tool calls
const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(30);

#[async_trait::async_trait]
pub trait ToolService: Send + Sync {
    async fn call(&self, call: ToolCallFull) -> ToolResult;
    fn list(&self) -> Vec<ToolDefinition>;
    fn usage_prompt(&self) -> String;
}

impl Service {
    pub fn tool_service() -> impl ToolService {
        Live::from_iter(forge_tool::tools())
    }
}

struct Live {
    tools: HashMap<ToolName, Tool>,
}

impl FromIterator<Tool> for Live {
    fn from_iter<T: IntoIterator<Item = Tool>>(iter: T) -> Self {
        let tools: HashMap<ToolName, Tool> = iter
            .into_iter()
            .map(|tool| (tool.definition.name.clone(), tool))
            .collect::<HashMap<_, _>>();

        Self { tools }
    }
}

#[async_trait::async_trait]
impl ToolService for Live {
    async fn call(&self, call: ToolCallFull) -> ToolResult {
        let name = call.name.clone();
        let input = call.arguments.clone();
        debug!("Calling tool: {}", name.as_str());
        let mut available_tools = self
            .tools
            .keys()
            .map(|name| name.as_str())
            .collect::<Vec<_>>();

        available_tools.sort();
        let output = match self.tools.get(&name) {
            Some(tool) => {
                // Wrap tool call with timeout
                match timeout(TOOL_CALL_TIMEOUT, tool.executable.call(input)).await {
                    Ok(result) => result,
                    Err(_) => Err(format!(
                        "Tool '{}' timed out after {} seconds",
                        name.as_str(),
                        TOOL_CALL_TIMEOUT.as_secs()
                    )),
                }
            }
            None => Err(format!(
                "No tool with name '{}' was found. Please try again with one of these tools {}",
                name.as_str(),
                available_tools.join(", ")
            )),
        };

        match output {
            Ok(output) => ToolResult::from(call).content(output),
            Err(output) => ToolResult::from(call).content(output).is_error(true),
        }
    }

    fn list(&self) -> Vec<ToolDefinition> {
        let mut tools: Vec<_> = self
            .tools
            .values()
            .map(|tool| tool.definition.clone())
            .collect();

        // Sorting is required to ensure system prompts are exactly the same
        tools.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        tools
    }

    fn usage_prompt(&self) -> String {
        let mut tools: Vec<_> = self.tools.values().collect();
        tools.sort_by(|a, b| a.definition.name.as_str().cmp(b.definition.name.as_str()));

        tools
            .iter()
            .enumerate()
            .fold("".to_string(), |mut acc, (i, tool)| {
                acc.push('\n');
                acc.push_str((i + 1).to_string().as_str());
                acc.push_str(". ");
                acc.push_str(tool.definition.usage_prompt().to_string().as_str());
                acc
            })
    }
}

#[cfg(test)]
mod test {
    use forge_domain::{Tool, ToolCallId, ToolDefinition};
    use serde_json::{json, Value};
    use tokio::time;

    use super::*;

    // Mock tool that always succeeds
    struct SuccessTool;
    #[async_trait::async_trait]
    impl forge_domain::ToolCallService for SuccessTool {
        type Input = Value;
        type Output = Value;

        async fn call(&self, input: Self::Input) -> Result<Self::Output, String> {
            Ok(Value::from(format!("Success with input: {}", input)))
        }
    }

    // Mock tool that always fails
    struct FailureTool;
    #[async_trait::async_trait]
    impl forge_domain::ToolCallService for FailureTool {
        type Input = Value;
        type Output = Value;

        async fn call(&self, _input: Self::Input) -> Result<Self::Output, String> {
            Err("Tool execution failed".to_string())
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

        Live::from_iter(vec![success_tool, failure_tool])
    }

    #[tokio::test]
    async fn test_successful_tool_call() {
        let service = new_tool_service();
        let call = ToolCallFull {
            name: ToolName::new("success_tool"),
            arguments: json!("test input"),
            call_id: Some(ToolCallId::new("test")),
        };

        let result = service.call(call).await;
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

        let result = service.call(call).await;
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

        let result = service.call(call).await;
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_tool_ids() {
        let service = Service::tool_service();
        let tools = service.list();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();

        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"search_in_files"));
        assert!(names.contains(&"list_directory_content"));
        assert!(names.contains(&"file_information"));
    }

    #[test]
    fn test_usage_prompt() {
        let service = Service::tool_service();
        let prompt = service.usage_prompt();

        assert!(!prompt.is_empty());
        assert!(prompt.contains("read_file"));
        assert!(prompt.contains("write_file"));
    }

    // Mock tool that simulates a long-running task
    struct SlowTool;
    #[async_trait::async_trait]
    impl forge_domain::ToolCallService for SlowTool {
        type Input = Value;
        type Output = Value;

        async fn call(&self, _input: Self::Input) -> Result<Self::Output, String> {
            // Simulate a long-running task that exceeds the timeout
            tokio::time::sleep(Duration::from_secs(40)).await;
            Ok(Value::from("Slow tool completed"))
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

        let service = Live::from_iter(vec![slow_tool]);
        let call = ToolCallFull {
            name: ToolName::new("slow_tool"),
            arguments: json!("test input"),
            call_id: Some(ToolCallId::new("test")),
        };

        // Advance time to trigger timeout
        test::time::advance(Duration::from_secs(35)).await;

        let result = service.call(call).await;

        // Assert that the result contains a timeout error message
        let content_str = result.content.as_str().unwrap_or("");

        assert!(
            content_str.contains("timed out"),
            "Expected timeout error message"
        );
        assert!(result.is_error, "Expected error result for timeout");
    }
}
