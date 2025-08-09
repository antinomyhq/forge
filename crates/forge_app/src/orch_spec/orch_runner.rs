use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Local;
use derive_setters::Setters;
use forge_domain::{
    Agent, AgentId, ChatCompletionMessage, ChatResponse, ContextMessage, Conversation,
    ConversationId, Environment, Event, HttpConfig, ModelId, RetryConfig, Role, Template,
    ToolCallFull, ToolResult, Workflow,
};
use handlebars::{Handlebars, no_escape};
use rust_embed::Embed;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;
use url::Url;

use crate::AgentService;
use crate::orch::Orchestrator;

#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

struct Runner {
    hb: Handlebars<'static>,
    // History of all the updates made to the conversation
    conversation_history: Mutex<Vec<Conversation>>,

    // Tool call requests and the mock responses
    test_tool_calls: Mutex<VecDeque<(ToolCallFull, ToolResult)>>,

    // Mock completions from the LLM (Each value is produced as an event in the stream)
    test_completions: Mutex<VecDeque<ChatCompletionMessage>>,
}

impl Runner {
    fn new(setup: &Setup) -> Self {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(no_escape);

        // Register all partial templates
        hb.register_embed_templates::<Templates>().unwrap();
        for (name, tpl) in &setup.templates {
            hb.register_template_string(name, tpl).unwrap();
        }

        Self {
            hb,
            conversation_history: Mutex::new(Vec::new()),
            test_tool_calls: Mutex::new(VecDeque::from(setup.mock_tool_call_responses.clone())),
            test_completions: Mutex::new(VecDeque::from(setup.mock_assistant_responses.clone())),
        }
    }

    // Returns the conversation history
    async fn get_history(&self) -> Vec<Conversation> {
        self.conversation_history.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl AgentService for Runner {
    async fn chat_agent(
        &self,
        _id: &forge_domain::ModelId,
        _context: forge_domain::Context,
    ) -> forge_domain::ResultStream<ChatCompletionMessage, anyhow::Error> {
        let mut responses = self.test_completions.lock().await;
        if let Some(message) = responses.pop_front() {
            Ok(Box::pin(tokio_stream::iter(std::iter::once(Ok(message)))))
        } else {
            Ok(Box::pin(tokio_stream::iter(std::iter::empty())))
        }
    }

    async fn call(
        &self,
        _agent: &forge_domain::Agent,
        _context: &mut forge_domain::ToolCallContext,
        test_call: forge_domain::ToolCallFull,
    ) -> forge_domain::ToolResult {
        let mut guard = self.test_tool_calls.lock().await;
        for (id, (call, result)) in guard.iter().enumerate() {
            if call.call_id == test_call.call_id {
                let result = result.clone();
                guard.remove(id);
                return result;
            }
        }
        panic!("Tool call not found")
    }

    async fn render(
        &self,
        template: &str,
        object: &(impl serde::Serialize + Sync),
    ) -> anyhow::Result<String> {
        Ok(self.hb.render_template(template, object)?)
    }

    async fn update(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.conversation_history.lock().await.push(conversation);
        Ok(())
    }
}

fn new_orchestrator(
    setup: &Setup,
    tx: Sender<anyhow::Result<ChatResponse>>,
) -> (Orchestrator<Runner>, Arc<Runner>) {
    let services = Arc::new(Runner::new(setup));
    let environment = new_env();
    let conversation = Conversation::new(
        ConversationId::generate(),
        setup.workflow.clone(),
        Default::default(),
    );
    let current_time = Local::now();

    let orch = Orchestrator::new(services.clone(), environment, conversation, current_time)
        .sender(Arc::new(tx))
        .files(setup.files.clone());

    // Return setup
    (orch, services)
}

fn new_env() -> Environment {
    Environment {
        os: "MacOS".to_string(),
        pid: 1234,
        cwd: PathBuf::from("/Users/tushar"),
        home: Some(PathBuf::from("/Users/tushar")),
        shell: "bash".to_string(),
        base_path: PathBuf::from("/Users/tushar/projects"),
        forge_api_url: Url::parse("http://localhost:8000").unwrap(),
        retry_config: RetryConfig::default(),
        max_search_lines: 1000,
        fetch_truncation_limit: 1024,
        stdout_max_prefix_length: 256,
        stdout_max_suffix_length: 256,
        max_read_size: 4096,
        http: HttpConfig::default(),
        max_file_size: 1024 * 1024 * 5,
        max_search_result_bytes: 200,
        stdout_max_line_length: 200, // 5 MB
    }
}

async fn run(setup: Setup) -> TestContext {
    const LIMIT: usize = 1024;
    let mut chat_responses = Vec::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<anyhow::Result<ChatResponse>>(LIMIT);
    let (mut orch, runner) = new_orchestrator(&setup, tx);

    tokio::join!(
        async { orch.chat(setup.event).await.unwrap() },
        rx.recv_many(&mut chat_responses, LIMIT)
    );
    TestContext {
        conversation_history: runner.get_history().await,
        chat_responses,
    }
}

// The final output produced after running the orchestrator to completion
#[derive(Debug)]
pub struct TestContext {
    pub conversation_history: Vec<Conversation>,
    pub chat_responses: Vec<anyhow::Result<ChatResponse>>,
}

impl TestContext {
    pub fn system_prompt(&self) -> Option<&str> {
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

#[derive(Setters)]
#[setters(into)]
pub struct Setup {
    pub event: Event,
    pub mock_tool_call_responses: Vec<(ToolCallFull, ToolResult)>,
    pub mock_assistant_responses: Vec<ChatCompletionMessage>,
    pub workflow: Workflow,
    pub templates: HashMap<String, String>,
    pub files: Vec<String>,
}

impl Setup {
    pub fn init_forge_task(task: &str) -> Self {
        Self::from_event(Event::new("forge/user_task_init", Some(task)))
    }

    pub fn from_event(event: Event) -> Self {
        Self {
            event,
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
        }
    }

    pub async fn run(self) -> TestContext {
        run(self).await
    }
}
