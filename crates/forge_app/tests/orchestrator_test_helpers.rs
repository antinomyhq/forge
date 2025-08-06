use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Local;
use derive_setters::Setters;
use forge_app::agent::AgentService;
use forge_app::orch::Orchestrator;
use forge_domain::{
    ChatCompletionMessage, Conversation, ConversationId, Environment, Event, HttpConfig,
    RetryConfig, Role, ToolCallFull, ToolResult, Workflow,
};
use handlebars::{Handlebars, no_escape};
use rust_embed::Embed;
use tokio::sync::Mutex;
use url::Url;

#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

#[derive(Setters, Debug)]
struct Runner {
    hb: Handlebars<'static>,
    // History of all the updates made to the conversation
    conversation_history: Mutex<Vec<Conversation>>,

    // Tool call requests and the mock responses
    test_tool_calls: Vec<(ToolCallFull, ToolResult)>,

    // Mock responses from the LLM (Each value is produced as an event in the stream)
    test_chat_responses: Mutex<VecDeque<ChatCompletionMessage>>,
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
            test_tool_calls: Vec::new(),
            test_chat_responses: Mutex::new(VecDeque::from(setup.mock_assistant_responses.clone())),
        }
    }

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
        let mut responses = self.test_chat_responses.lock().await;
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
        self.test_tool_calls
            .iter()
            .find(|(call, _)| call.call_id == test_call.call_id)
            .map(|(_, result)| result.clone())
            .expect("Tool call not found")
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

fn new_orchestrator(setup: &Setup) -> (Orchestrator<Runner>, Arc<Runner>) {
    let services = new_service(setup);
    let environment = new_env();
    let conversation = new_conversation(&setup.workflow);
    let current_time = new_current_time();
    (
        Orchestrator::new(services.clone(), environment, conversation, current_time)
            .files(setup.files.clone()),
        services,
    )
}

fn new_current_time() -> chrono::DateTime<Local> {
    Local::now()
}

fn new_service(setup: &Setup) -> Arc<Runner> {
    Arc::new(Runner::new(setup))
}

fn new_conversation(workflow: &Workflow) -> Conversation {
    Conversation::new(
        ConversationId::generate(),
        workflow.clone(),
        Default::default(),
    )
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
    let (mut orch, services) = new_orchestrator(&setup);
    orch.chat(setup.event).await.unwrap();
    TestContext { conversation_history: services.get_history().await }
}

// The final output produced after running the orchestrator to completion
pub struct TestContext {
    pub conversation_history: Vec<Conversation>,
}

impl TestContext {
    pub fn system_prompt(&self) -> Option<&str> {
        self.conversation_history
            .last()
            .and_then(|c| c.context.as_ref())
            .and_then(|c| c.messages.iter().find(|c| c.has_role(Role::System)))
            .and_then(|c| c.content())
    }
}

#[derive(Setters)]
#[setters(into)]
pub struct Setup {
    pub event: Event,
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
            workflow: Default::default(),
            templates: Default::default(),
            files: Default::default(),
        }
    }

    pub async fn run(self) -> TestContext {
        run(self).await
    }
}
