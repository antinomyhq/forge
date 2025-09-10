use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Local};
use derive_setters::Setters;
use forge_domain::{
    Agent, AgentId, ChatCompletionMessage, ChatResponse, ContextMessage, Conversation, Environment,
    Event, HttpConfig, ModelId, RetryConfig, Role, Template, ToolCallFull, ToolDefinition,
    ToolResult, Workflow,
};
use url::Url;

use crate::orch_spec::orch_runner::Runner;

#[derive(Setters)]
#[setters(into)]
pub struct TestContext {
    pub event: Event,
    pub mock_tool_call_responses: Vec<(ToolCallFull, ToolResult)>,
    pub mock_assistant_responses: Vec<ChatCompletionMessage>,
    pub workflow: Workflow,
    pub templates: HashMap<String, String>,
    pub files: Vec<String>,
    pub env: Environment,
    pub current_time: DateTime<Local>,

    // Final output of the test is store in the context
    pub output: TestOutput,
    pub agent: Agent,
    pub tools: Vec<ToolDefinition>,
}

impl TestContext {
    pub fn init_forge_task(task: &str) -> Self {
        Self::from_event(Event::new("forge/user_task_init", Some(task)))
    }

    pub fn from_event(event: Event) -> Self {
        Self {
            event,
            output: TestOutput::default(),
            current_time: Local::now(),
            mock_assistant_responses: Default::default(),
            mock_tool_call_responses: Default::default(),
            workflow: Workflow::new()
                .model(ModelId::new("openai/gpt-1"))
                .tool_supported(true),
            templates: Default::default(),
            files: Default::default(),
            env: Environment {
                os: "MacOS".to_string(),
                pid: 1234,
                cwd: PathBuf::from("/Users/tushar"),
                home: Some(PathBuf::from("/Users/tushar")),
                shell: "bash".to_string(),
                base_path: PathBuf::from("/Users/tushar/projects"),
                forge_api_url: Url::parse("http://localhost:8000").unwrap(),

                // No retry policy by default
                retry_config: RetryConfig {
                    initial_backoff_ms: 0,
                    min_delay_ms: 0,
                    backoff_factor: 0,
                    max_retry_attempts: 0,
                    retry_status_codes: Default::default(),
                    max_delay: Default::default(),
                    suppress_retry_errors: Default::default(),
                },
                tool_timeout: 300,
                max_search_lines: 1000,
                fetch_truncation_limit: 1024,
                stdout_max_prefix_length: 256,
                stdout_max_suffix_length: 256,
                max_read_size: 4096,
                http: HttpConfig::default(),
                max_file_size: 1024 * 1024 * 5,
                max_search_result_bytes: 200,
                stdout_max_line_length: 200, // 5 MB
            },
            agent: Agent::new(AgentId::new("forge"))
                .system_prompt(Template::new("You are Forge"))
                .tools(vec![("fs_read").into(), ("fs_write").into()]),
            tools: vec![
                ToolDefinition::new("fs_read"),
                ToolDefinition::new("fs_write"),
            ],
        }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        Runner::run(self).await
    }
}

// The final output produced after running the orchestrator to completion
#[derive(Default, Debug)]
pub struct TestOutput {
    pub conversation_history: Vec<Conversation>,
    pub chat_responses: Vec<anyhow::Result<ChatResponse>>,
}

impl TestOutput {
    pub fn system_messages(&self) -> Option<Vec<&str>> {
        self.conversation_history
            .last()
            .and_then(|c| c.context.as_ref())
            .and_then(|c| {
                c.messages
                    .iter()
                    .filter(|c| c.has_role(Role::System))
                    .map(|m| m.content())
                    .collect()
            })
    }

    pub fn context_messages(&self) -> Vec<ContextMessage> {
        self.conversation_history
            .last()
            .and_then(|c| c.context.as_ref())
            .map(|c| c.messages.clone())
            .clone()
            .unwrap_or_default()
    }
}
