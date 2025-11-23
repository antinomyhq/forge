use std::collections::VecDeque;
use std::sync::Arc;

use forge_domain::{
    Attachment, ChatCompletionMessage, ChatResponse, Conversation, ConversationId, Event,
    ProviderId, ToolCallFull, ToolErrorTracker, ToolResult,
};
use handlebars::{Handlebars, no_escape};
use rust_embed::Embed;
use tokio::sync::Mutex;

pub use super::orch_setup::TestContext;
use crate::apply_tunable_parameters::ApplyTunableParameters;
use crate::init_conversation_metrics::InitConversationMetrics;
use crate::orch::Orchestrator;
use crate::set_conversation_id::SetConversationId;
use crate::system_prompt::SystemPrompt;
use crate::user_prompt::UserPromptGenerator;
use crate::{AgentService, AttachmentService, ConcreteInlineShellExecutor, TemplateService};

#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

pub struct Runner {
    hb: Handlebars<'static>,
    // History of all the updates made to the conversation
    conversation_history: Mutex<Vec<Conversation>>,

    // Tool call requests and the mock responses
    test_tool_calls: Mutex<VecDeque<(ToolCallFull, ToolResult)>>,

    // Mock completions from the LLM (Each value is produced as an event in the stream)
    test_completions: Mutex<VecDeque<ChatCompletionMessage>>,

    attachments: Vec<Attachment>,
    prompt_processor: Arc<crate::ForgePromptProcessor>,
}

impl Runner {
    fn prompt_processor(&self) -> Arc<crate::ForgePromptProcessor> {
        Arc::clone(&self.prompt_processor)
    }

    fn new(setup: &TestContext) -> Self {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(no_escape);

        // Register all partial templates
        hb.register_embed_templates::<Templates>().unwrap();
        for (name, tpl) in &setup.templates {
            hb.register_template_string(name, tpl).unwrap();
        }

        // Create inline shell executor - for testing we need a simple infrastructure
        // mock Since we can't access the test mock, we'll create a minimal one
        // here
        use std::collections::HashMap;

        struct SimpleCommandInfra {
            responses: HashMap<String, forge_domain::CommandOutput>,
        }

        impl SimpleCommandInfra {
            fn new() -> Self {
                let mut responses = HashMap::new();
                responses.insert(
                    "date".to_string(),
                    forge_domain::CommandOutput {
                        command: "date".to_string(),
                        stdout: "2025-01-01".to_string(),
                        stderr: String::new(),
                        exit_code: Some(0),
                    },
                );
                responses.insert(
                    "pwd".to_string(),
                    forge_domain::CommandOutput {
                        command: "pwd".to_string(),
                        stdout: "/test".to_string(),
                        stderr: String::new(),
                        exit_code: Some(0),
                    },
                );
                Self { responses }
            }
        }

        #[async_trait::async_trait]
        impl crate::infra::CommandInfra for SimpleCommandInfra {
            async fn execute_command(
                &self,
                command: String,
                _working_dir: std::path::PathBuf,
                _silent: bool,
                _env_vars: Option<Vec<String>>,
            ) -> Result<forge_domain::CommandOutput, anyhow::Error> {
                // Return mock response for known commands
                if let Some(output) = self.responses.get(&command) {
                    Ok(forge_domain::CommandOutput {
                        command: output.command.clone(),
                        stdout: output.stdout.clone(),
                        stderr: output.stderr.clone(),
                        exit_code: output.exit_code,
                    })
                } else {
                    // Default response for unknown commands
                    Ok(forge_domain::CommandOutput {
                        command: command.clone(),
                        stdout: String::new(),
                        stderr: format!("Command not found: {}", command),
                        exit_code: Some(1),
                    })
                }
            }

            async fn execute_command_raw(
                &self,
                command: &str,
                _working_dir: std::path::PathBuf,
                _env_vars: Option<Vec<String>>,
            ) -> Result<std::process::ExitStatus, anyhow::Error> {
                use std::os::unix::process::ExitStatusExt;
                use std::process::ExitStatus;

                // For mock, just return a successful exit status for known commands
                if self.responses.contains_key(command) {
                    Ok(ExitStatus::from_raw(0))
                } else {
                    Ok(ExitStatus::from_raw(1))
                }
            }
        }

        let inline_shell_executor = Arc::new(ConcreteInlineShellExecutor::new(
            std::sync::Arc::new(SimpleCommandInfra::new()),
            forge_domain::Environment::default(),
        ));

        // Create PromptProcessor for testing
        use crate::{ForgePromptProcessor, SecurityValidationService};
        let security_validation_service = SecurityValidationService::new();
        let prompt_processor = Arc::new(ForgePromptProcessor::new(
            security_validation_service,
            inline_shell_executor.clone(),
        ));

        Self {
            hb,
            attachments: setup.attachments.clone(),
            conversation_history: Mutex::new(Vec::new()),
            test_tool_calls: Mutex::new(VecDeque::from(setup.mock_tool_call_responses.clone())),
            test_completions: Mutex::new(VecDeque::from(setup.mock_assistant_responses.clone())),
            prompt_processor,
        }
    }

    // Returns the conversation history
    async fn get_history(&self) -> Vec<Conversation> {
        self.conversation_history.lock().await.clone()
    }

    pub async fn run(setup: &mut TestContext, event: Event) -> anyhow::Result<()> {
        const LIMIT: usize = 1024;
        let (tx, mut rx) = tokio::sync::mpsc::channel::<anyhow::Result<ChatResponse>>(LIMIT);
        let handle = tokio::spawn(async move {
            let mut responses = Vec::new();

            while let Some(item) = rx.recv().await {
                responses.push(item);
            }

            responses
        });

        let services = Arc::new(Runner::new(setup));
        // setup the conversation
        let conversation = Conversation::new(ConversationId::generate()).title(setup.title.clone());

        let agent = setup.agent.clone();
        let system_tools = setup.tools.clone();
        let agent = agent
            .apply_workflow_config(&setup.workflow)
            .model(setup.model.clone());

        // Render system prompt into context.
        let conversation = SystemPrompt::new(
            services.clone(),
            setup.env.clone(),
            agent.clone(),
            services.prompt_processor(),
        )
        .files(setup.files.clone())
        .tool_definitions(system_tools.clone())
        .add_system_message(conversation)
        .await?;

        // Render user prompt into context.
        let conversation = UserPromptGenerator::new(
            services.clone(),
            agent.clone(),
            event.clone(),
            setup.current_time,
        )
        .add_user_prompt(conversation)
        .await?;

        let conversation = InitConversationMetrics::new(setup.current_time).apply(conversation);
        let conversation =
            ApplyTunableParameters::new(agent.clone(), system_tools.clone()).apply(conversation);
        let conversation = SetConversationId.apply(conversation);

        let orch = Orchestrator::new(
            services.clone(),
            setup.env.clone(),
            conversation,
            agent,
            event,
        )
        .error_tracker(ToolErrorTracker::new(3))
        .tool_definitions(system_tools)
        .sender(tx);

        let (mut orch, runner) = (orch, services);

        let result = orch.run().await;
        drop(orch);

        let chat_responses = handle.await?;

        setup.output.chat_responses.extend(chat_responses);
        setup
            .output
            .conversation_history
            .extend(runner.get_history().await);

        result
    }
}

#[async_trait::async_trait]
impl AgentService for Runner {
    async fn chat_agent(
        &self,
        _id: &forge_domain::ModelId,
        context: forge_domain::Context,
        _provider_id: Option<ProviderId>,
    ) -> forge_domain::ResultStream<ChatCompletionMessage, anyhow::Error> {
        let mut responses = self.test_completions.lock().await;

        if let Some(message) = responses.pop_front() {
            Ok(Box::pin(tokio_stream::iter(std::iter::once(Ok(message)))))
        } else {
            let total_messages = context.messages.len();
            let last_message = context.messages.last();
            panic!(
                "No mock response found. Total Messages: {total_messages}. Last Message: {last_message:#?}"
            )
        }
    }

    async fn call(
        &self,
        _: &forge_domain::Agent,
        _: &forge_domain::ToolCallContext,
        test_call: forge_domain::ToolCallFull,
    ) -> forge_domain::ToolResult {
        let name = test_call.name.clone();
        let mut guard = self.test_tool_calls.lock().await;
        for (id, (call, result)) in guard.iter().enumerate() {
            if call.call_id == test_call.call_id {
                let result = result.clone();
                guard.remove(id);
                return result;
            }
        }

        panic!("No mock tool call not found: {name}")
    }

    async fn update(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.conversation_history.lock().await.push(conversation);
        Ok(())
    }
}

#[async_trait::async_trait]
impl TemplateService for Runner {
    async fn register_template(&self, _path: std::path::PathBuf) -> anyhow::Result<()> {
        unimplemented!()
    }

    async fn render_template<V: serde::Serialize + Send + Sync>(
        &self,
        template: forge_domain::Template<V>,
        object: &V,
    ) -> anyhow::Result<String> {
        Ok(self.hb.render_template(&template.template, object)?)
    }
}

#[async_trait::async_trait]
impl AttachmentService for Runner {
    async fn attachments(&self, _url: &str) -> anyhow::Result<Vec<forge_domain::Attachment>> {
        Ok(self.attachments.clone())
    }
}
