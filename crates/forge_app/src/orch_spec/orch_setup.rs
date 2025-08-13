use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Local};
use derive_setters::Setters;
use forge_domain::{
    Agent, AgentId, ChatCompletionMessage, ChatResponse, ContextMessage, Conversation, Environment,
    Event, HttpConfig, ModelId, RetryConfig, Role, Template, ToolCallFull, ToolResult, Workflow,
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
                .agents(vec![
                    Agent::new(AgentId::new("forge"))
                        .system_prompt(Template::new("You are Forge"))
                        .tools(vec![("fs_read").into(), ("fs_write").into()]),
                    Agent::new(AgentId::new("must"))
                        .system_prompt(Template::new("You are Muse"))
                        .tools(vec![("fs_read").into()]),
                ])
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
        }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        Runner::run(self).await
    }

    // Helper utility functions for message counting and checking
    pub fn count(&self, text: &str) -> usize {
        self.output
            .context_messages()
            .iter()
            .filter_map(|message| message.content())
            .filter(|content| content.contains(text))
            .count()
    }

    pub fn has_message_containing(&self, text: &str) -> bool {
        self.output
            .context_messages()
            .iter()
            .filter_map(|message| message.content())
            .any(|content| content.contains(text))
    }

    pub fn has_complete_retry_info(&self, attempts_remaining: &str, max_attempts: &str) -> bool {
        self.output
            .context_messages()
            .iter()
            .filter_map(|message| message.content())
            .any(|content| {
                content.contains("This tool call failed")
                    && content.contains(&format!(
                        "You have {} attempt(s) remaining",
                        attempts_remaining
                    ))
                    && content.contains(&format!("out of a maximum of {}", max_attempts))
                    && content.contains("Please reflect on the error")
            })
    }
}

// The final output produced after running the orchestrator to completion
#[derive(Default, Debug)]
pub struct TestOutput {
    pub conversation_history: Vec<Conversation>,
    pub chat_responses: Vec<anyhow::Result<ChatResponse>>,
}

impl TestOutput {
    pub fn system_prompt(&self) -> Option<String> {
        self.conversation_history
            .last()
            .and_then(|c| c.context.as_ref())
            .and_then(|c| c.messages.iter().find(|c| c.has_role(Role::System)))
            .and_then(|c| c.content())
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
