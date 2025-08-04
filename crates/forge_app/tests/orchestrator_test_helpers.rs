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
pub struct TestAgentServices {
    hb: Handlebars<'static>,
    history: Mutex<Vec<Conversation>>,
    test_tool_calls: Vec<(ToolCallFull, ToolResult)>,
    test_chat_responses: Mutex<VecDeque<ChatCompletionMessage>>,
}

impl TestAgentServices {
    pub fn new(messages: Vec<ChatCompletionMessage>) -> Self {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(no_escape);

        // Register all partial templates
        hb.register_embed_templates::<Templates>().unwrap();
        Self {
            hb,
            history: Mutex::new(Vec::new()),
            test_tool_calls: Vec::new(),
            test_chat_responses: Mutex::new(VecDeque::from(messages)),
        }
    }

    pub async fn get_history(&self) -> Option<Conversation> {
        let conversation = self.history.lock().await;
        conversation.last().cloned()
    }
}

#[async_trait::async_trait]
impl AgentService for TestAgentServices {
    async fn chat_agent(
        &self,
        _id: &forge_domain::ModelId,
        _context: forge_domain::Context,
    ) -> forge_domain::ResultStream<ChatCompletionMessage, anyhow::Error> {
        let mut responses = self.test_chat_responses.lock().await;
        if let Some(message) = responses.pop_front() {
            Ok(Box::pin(tokio_stream::iter(std::iter::once(Ok(message)))))
        } else {
            Ok(Box::pin(tokio_stream::iter(std::iter::once(Err(
                anyhow::anyhow!("No more test chat responses available"),
            )))))
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
        self.history.lock().await.push(conversation);
        Ok(())
    }
}

fn new_orchestrator(
    messages: &[ChatCompletionMessage],
) -> (Orchestrator<TestAgentServices>, Arc<TestAgentServices>) {
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

fn new_service(messages: Vec<ChatCompletionMessage>) -> Arc<TestAgentServices> {
    Arc::new(TestAgentServices::new(messages))
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

pub async fn run(messages: &[ChatCompletionMessage]) -> Arc<TestAgentServices> {
    let (mut orch, services) = new_orchestrator(messages);
    orch.chat(Event::new("forge/user_task_init", Some("This is a test")))
        .await
        .unwrap();
    services
}
