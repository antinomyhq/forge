use std::path::PathBuf;
use std::sync::Arc;

use chrono::Local;
use derive_setters::Setters;
use forge_app::agent::AgentService;
use forge_app::orch::Orchestrator;
use forge_domain::{
    ChatCompletionMessage, Conversation, ConversationId, Environment, HttpConfig, RetryConfig,
    ToolCallFull, ToolResult, Workflow,
};
use handlebars::{Handlebars, no_escape};
use rust_embed::Embed;
use tokio::sync::Mutex;
use url::Url;

#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

#[derive(Setters, Debug)]
pub struct OrchestratorServices {
    hb: Handlebars<'static>,
    history: Mutex<Vec<Conversation>>,
    test_tool_calls: Vec<(ToolCallFull, ToolResult)>,
    test_chat_responses: Vec<ChatCompletionMessage>,
}

impl OrchestratorServices {
    pub fn new() -> Self {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(no_escape);

        // Register all partial templates
        hb.register_embed_templates::<Templates>().unwrap();
        Self {
            hb,
            history: Mutex::new(Vec::new()),
            test_tool_calls: Vec::new(),
            test_chat_responses: Vec::new(),
        }
    }

    pub fn add_test_tool_call(&mut self, tool_call: ToolCallFull, tool_result: ToolResult) {
        // Logic to handle tool calls and results
        // This is a placeholder for actual implementation
        self.test_tool_calls.push((tool_call, tool_result));
    }
}

#[async_trait::async_trait]
impl AgentService for OrchestratorServices {
    async fn chat_agent(
        &self,
        _id: &forge_domain::ModelId,
        _context: forge_domain::Context,
    ) -> forge_domain::ResultStream<ChatCompletionMessage, anyhow::Error> {
        Ok(Box::pin(tokio_stream::iter(
            self.test_chat_responses.clone().into_iter().map(Ok),
        )))
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
        self.hb
            .render(template, object)
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn update(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.history.lock().await.push(conversation);
        Ok(())
    }
}

fn new_orchestrator() -> Orchestrator<OrchestratorServices> {
    let services = new_service();
    let environment = new_env();
    let workflow = new_workflow();
    let conversation = new_conversation(workflow);
    let current_time = new_current_time();
    Orchestrator::new(services, environment, conversation, current_time)
}

fn new_current_time() -> chrono::DateTime<Local> {
    Local::now()
}

fn new_service() -> Arc<OrchestratorServices> {
    Arc::new(OrchestratorServices::new())
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

#[test]
fn test_orchestrator_creation() {
    let _orch = new_orchestrator();
    // Test that orchestrator can be created successfully
    assert!(true);
}
