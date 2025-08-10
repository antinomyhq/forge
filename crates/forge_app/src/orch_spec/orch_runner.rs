use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Local;
use forge_domain::{
    ChatCompletionMessage, ChatResponse, ContextMessage, Conversation, ConversationId, Environment,
    HttpConfig, RetryConfig, Role, ToolCallFull, ToolResult,
};
use handlebars::{Handlebars, no_escape};
use rust_embed::Embed;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;
use url::Url;

pub use super::orch_setup::Setup;
use crate::AgentService;
use crate::orch::Orchestrator;
use crate::orch_spec::orch_setup::TestContext;

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
    let conversation = Conversation::new(
        ConversationId::generate(),
        setup.workflow.clone(),
        Default::default(),
    );
    let current_time = Local::now();

    let orch = Orchestrator::new(
        services.clone(),
        setup.env.clone(),
        conversation,
        current_time,
    )
    .sender(Arc::new(tx))
    .files(setup.files.clone());

    // Return setup
    (orch, services)
}

pub async fn run(setup: Setup) -> TestContext {
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
