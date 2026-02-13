use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Local};
use derive_setters::Setters;
use forge_domain::{
    Agent, AgentId, Attachment, ChatCompletionMessage, ChatResponse, Conversation, Environment,
    Event, File, HttpConfig, MessageEntry, ModelId, ProviderId, RetryConfig, Role, Template,
    ToolCallFull, ToolDefinition, ToolResult, Workflow,
};
use url::Url;

use crate::orch_spec::orch_runner::Runner;

// User prompt
const USER_PROMPT: &str = r#"
  <{{event.name}}>{{event.value}}</{{event.name}}>
  <system_date>{{current_date}}</system_date>
"#;

#[derive(Setters)]
#[setters(into)]
pub struct TestContext {
    pub mock_tool_call_responses: Vec<(ToolCallFull, ToolResult)>,
    pub mock_assistant_responses: Vec<ChatCompletionMessage>,
    pub workflow: Workflow,
    pub templates: HashMap<String, String>,
    pub files: Vec<File>,
    pub env: Environment,
    pub current_time: DateTime<Local>,
    pub title: Option<String>,
    pub model: ModelId,
    pub attachments: Vec<Attachment>,

    // Final output of the test is store in the context
    pub output: TestOutput,
    pub agent: Agent,
    pub tools: Vec<ToolDefinition>,
}

impl Default for TestContext {
    fn default() -> Self {
        Self {
            model: ModelId::new("openai/gpt-1"),
            output: TestOutput::default(),
            current_time: Local::now(),
            mock_assistant_responses: Default::default(),
            mock_tool_call_responses: Default::default(),
            workflow: Workflow::new().tool_supported(true),
            templates: Default::default(),
            files: Default::default(),
            attachments: Default::default(),
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
                mcp_truncation_limit: 1024,
                stdout_max_prefix_length: 256,
                stdout_max_suffix_length: 256,
                max_read_size: 4096,
                http: HttpConfig::default(),
                max_file_size: 1024 * 1024 * 5,
                max_search_result_bytes: 200,
                stdout_max_line_length: 200, // 5 MB
                max_line_length: 2000,
                auto_open_dump: false,
                debug_requests: None,
                custom_history_path: None,
                max_conversations: 100,
                sem_search_limit: 100,
                sem_search_top_k: 10,
                max_image_size: 262144,
                workspace_server_url: Url::parse("http://localhost:8080").unwrap(),
                override_model: None,
                override_provider: None,
            },
            title: Some("test-conversation".into()),
            agent: Agent::new(
                AgentId::new("forge"),
                ProviderId::ANTHROPIC,
                ModelId::new("claude-3-5-sonnet-20241022"),
            )
            .system_prompt(Template::new("You are Forge"))
            .user_prompt(Template::new(USER_PROMPT))
            .tools(vec![("fs_read").into(), ("fs_write").into()]),
            tools: vec![
                ToolDefinition::new("fs_read"),
                ToolDefinition::new("fs_write"),
            ],
        }
    }
}

impl TestContext {
    pub async fn run(&mut self, event: impl AsRef<str>) -> anyhow::Result<()> {
        self.run_event(Event::new(event.as_ref())).await
    }

    pub async fn run_event(&mut self, event: impl Into<Event>) -> anyhow::Result<()> {
        Runner::run(self, event.into()).await
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

    pub fn context_messages(&self) -> Vec<MessageEntry> {
        self.conversation_history
            .last()
            .and_then(|c| c.context.as_ref())
            .map(|c| c.messages.clone())
            .clone()
            .unwrap_or_default()
    }

    pub fn tools(&self) -> Vec<ToolDefinition> {
        self.conversation_history
            .last()
            .and_then(|c| c.context.as_ref())
            .map(|c| c.tools.clone())
            .clone()
            .unwrap_or_default()
    }
}
