use std::collections::HashMap;

use derive_builder::Builder;
use derive_setters::Setters;
use forge_domain::{Agent, Environment, Model, SystemContext, ToolDefinition, ToolUsagePrompt};

use crate::neo_orch::events::{AgentAction, AgentCommand};
use crate::neo_orch::program::{Program, ProgramExt};
use crate::neo_orch::programs::SystemPromptProgramBuilder;
use crate::neo_orch::programs::attachment_program::AttachmentProgramBuilder;
use crate::neo_orch::programs::chat_program::ChatProgramBuilder;
use crate::neo_orch::programs::init_tool_program::InitToolProgramBuilder;
use crate::neo_orch::programs::user_prompt_program::UserPromptProgramBuilder;
use crate::neo_orch::state::AgentState;

///
/// The main agent program that runs an agent
#[derive(Setters, Builder)]
#[setters(strip_option, into)]
pub struct AgentProgram {
    tool_definitions: Vec<ToolDefinition>,
    agent: Agent,
    model: Model,
    environment: Environment,
    files: Vec<String>,
    current_time: chrono::DateTime<chrono::Local>,
}

impl Program for AgentProgram {
    type State = AgentState;
    type Action = AgentAction;
    type Success = AgentCommand;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        // Create proper SystemContext like the old orchestrator does
        let mut files = self.files.clone();
        files.sort();

        let current_time = self
            .current_time
            .format("%Y-%m-%d %H:%M:%S %:z")
            .to_string();

        // Check if agent supports tools (simplified version)
        let tool_supported = self
            .agent
            .tool_supported
            .unwrap_or(self.model.tools_supported.unwrap_or_default());

        let supports_parallel_tool_calls =
            self.model.supports_parallel_tool_calls.unwrap_or_default();

        let tool_information = match tool_supported {
            true => None,
            false => Some(ToolUsagePrompt::from(&self.tool_definitions).to_string()),
        };

        let system_context = SystemContext {
            current_time,
            env: Some(self.environment.clone()),
            tool_information,
            tool_supported,
            files,
            custom_rules: self
                .agent
                .custom_rules
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            variables: HashMap::new(),
            supports_parallel_tool_calls,
        };

        let program = InitToolProgramBuilder::default()
            .tool_definitions(self.tool_definitions.clone())
            .agent(self.agent.clone())
            .build()?
            .combine(
                SystemPromptProgramBuilder::default()
                    .system_prompt(self.agent.system_prompt.clone())
                    .context(Some(system_context))
                    .build()?,
            )
            .combine(
                UserPromptProgramBuilder::default()
                    .agent(self.agent.clone())
                    .variables(HashMap::new())
                    .current_time(chrono::Utc::now().to_rfc3339())
                    .pending_event(None)
                    .build()?,
            )
            .combine(
                AttachmentProgramBuilder::default()
                    .model_id(self.model.id.clone())
                    .build()?,
            )
            .combine(
                ChatProgramBuilder::default()
                    .model_id(self.model.id.clone())
                    .build()?,
            );

        program.update(action, state)
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Agent, AgentId, Event, Model, ModelId, ToolDefinition};

    use super::*;
    use crate::neo_orch::events::AgentAction;
    use crate::neo_orch::program::Program;
    use crate::neo_orch::state::AgentState;

    fn create_test_agent_program() -> AgentProgram {
        let tool_definitions = vec![ToolDefinition::new("test_tool").description("A test tool")];
        let agent = Agent::new(AgentId::new("test-agent"));
        let model = Model {
            id: ModelId::new("test-model"),
            name: None,
            description: None,
            context_length: None,
            tools_supported: None,
            supports_parallel_tool_calls: None,
            supports_reasoning: None,
        };
        let environment = Environment {
            os: "linux".to_string(),
            pid: 1234,
            cwd: std::path::PathBuf::from("/test"),
            home: Some(std::path::PathBuf::from("/home/test")),
            shell: "/bin/bash".to_string(),
            base_path: std::path::PathBuf::from("/test"),
            forge_api_url: "http://localhost:8080".parse().unwrap(),
            retry_config: Default::default(),
            max_search_lines: 1000,
            fetch_truncation_limit: 10000,
            stdout_max_prefix_length: 100,
            stdout_max_suffix_length: 100,
            max_read_size: 1000000,
            http: Default::default(),
            max_file_size: 1000000,
        };
        let files = vec!["test.rs".to_string()];
        let current_time = chrono::Local::now();

        AgentProgramBuilder::default()
            .tool_definitions(tool_definitions)
            .agent(agent)
            .model(model)
            .environment(environment)
            .files(files)
            .current_time(current_time)
            .build()
            .unwrap()
    }

    #[test]
    fn test_update_handles_chat_event() {
        let fixture = create_test_agent_program();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("Hello world")));

        let actual = fixture.update(&action, &mut state);

        assert!(
            actual.is_ok(),
            "Expected update to succeed, but got error: {:?}",
            actual.err()
        );
    }
}
