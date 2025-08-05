use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Local;
use derive_setters::Setters;
use forge_app::agent::AgentService;
use forge_app::orch::Orchestrator;
use forge_domain::{
    ChatCompletionMessage, Conversation, ConversationId, Environment, Event, HttpConfig,
    RetryConfig, ToolCallFull, ToolResult, Workflow,
};
use handlebars::{Handlebars, no_escape};
use rust_embed::Embed;
use tokio::sync::Mutex;
use url::Url;

#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

#[derive(Setters, Debug)]
pub struct Trace {
    hb: Mutex<Handlebars<'static>>,
    history: Mutex<Vec<Conversation>>,
    test_tool_calls: Vec<(ToolCallFull, ToolResult)>,
    test_chat_responses: Mutex<VecDeque<ChatCompletionMessage>>,
}

impl Trace {
    fn new(messages: Vec<ChatCompletionMessage>) -> Self {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(no_escape);

        // Register all partial templates
        hb.register_embed_templates::<Templates>().unwrap();
        Self {
            hb: Mutex::new(hb),
            history: Mutex::new(Vec::new()),
            test_tool_calls: Vec::new(),
            test_chat_responses: Mutex::new(VecDeque::from(messages)),
        }
    }

    pub async fn register_template(&self, name: &str, template: &str) -> anyhow::Result<()> {
        let mut guard = self.hb.lock().await;
        guard.register_template_string(name, template)?;
        Ok(())
    }

    pub async fn get_history(&self) -> Vec<Conversation> {
        self.history.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl AgentService for Trace {
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
        let guard = self.hb.lock().await;
        Ok(guard.render_template(template, object)?)
    }

    async fn update(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.history.lock().await.push(conversation);
        Ok(())
    }
}

fn new_orchestrator(messages: Vec<ChatCompletionMessage>) -> (Orchestrator<Trace>, Arc<Trace>) {
    let services = new_service(messages.to_vec());
    let environment = new_env();
    let workflow = new_workflow();
    let conversation = new_conversation(workflow);
    let current_time = new_current_time();
    (
        Orchestrator::new(services.clone(), environment, conversation, current_time),
        services,
    )
}

fn new_current_time() -> chrono::DateTime<Local> {
    Local::now()
}

fn new_service(messages: Vec<ChatCompletionMessage>) -> Arc<Trace> {
    Arc::new(Trace::new(messages))
}

fn new_workflow() -> Workflow {
    Workflow::default()
}

fn new_conversation(workflow: Workflow) -> Conversation {
    Conversation::new(ConversationId::generate(), workflow, Default::default())
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
        max_file_size: 1024 * 1024 * 5, // 5 MB
    }
}

pub struct Setup {
    pub orch: Orchestrator<Trace>,
    pub services: Arc<Trace>,
}

impl Default for Setup {
    fn default() -> Self {
        let (orch, services) = new_orchestrator(vec![]);
        Self { orch, services }
    }
}

impl Setup {
    pub fn add_agent(mut self, agent: forge_domain::Agent) -> Self {
        let mut conversation = self.orch.get_conversation().clone();
        conversation.agents.push(agent);
        self.orch = self.orch.conversation(conversation);
        self
    }

    pub async fn system_prompt(&self) -> Option<String> {
        let guard = self.services.history.lock().await;
        guard
            .last()
            .and_then(|conv| conv.context.as_ref().and_then(|ctx| ctx.system_prompt()))
            .map(|sp| sp.to_string())
    }

    pub async fn chat(&mut self, input: String, agent_id: String) -> anyhow::Result<()> {
        self.orch
            .chat(Event::new(
                format!("{}/user_task_init", agent_id),
                Some(input),
            ))
            .await
    }
}
