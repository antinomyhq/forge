//! Default configuration for Forge
//!
//! This module contains the default configuration that is used when no
//! custom configuration is provided.

use std::collections::HashMap;

use forge_domain::{
    Agent, AgentId, EventContext, ModelId, SystemContext, Template, ToolName, Workflow,
};
use serde_json::json;

/// The original default forge.yaml content as a string constant.
/// Kept for reference and backwards compatibility.
pub const DEFAULT_FORGE_YAML: &str = r#"# YAML Variables
advanced_model: &advanced_model anthropic/claude-3.7-sonnet
efficiency_model: &efficiency_model anthropic/claude-3.5-haiku

variables:
  mode: ACT
agents:
  - id: title_generation_worker
    model: *efficiency_model
    tool_supported: true
    tools:
      - tool_forge_event_dispatch
    subscribe:
      - user_task_init
    system_prompt: "{{> system-prompt-title-generator.hbs }}"
    user_prompt: <technical_content>{{event.value}}</technical_content>

  - id: help_agent
    model: google/gemini-2.0-flash-thinking-exp:free
    tools:
      - tool_forge_fs_read
      - tool_forge_fs_create
    subscribe:
      - user_help_query
    system_prompt: |
      {{> system-prompt-help.hbs }}
    user_prompt: <query>{{event.value}}</query>

  - id: software-engineer
    model: *advanced_model
    tool_supported: true
    tools:
      - tool_forge_fs_read
      - tool_forge_fs_create
      - tool_forge_fs_remove
      - tool_forge_fs_patch
      - tool_forge_process_shell
      - tool_forge_net_fetch
      - tool_forge_fs_search
    subscribe:
      - user_task_init
      - user_task_update
    ephemeral: false
    max_walker_depth: 4
    system_prompt: "{{> system-prompt-engineer.hbs }}"
    user_prompt: |
      <task>{{event.value}}</task>
      <mode>{{variables.mode}}</mode>
"#;

/// System prompt templates for each agent type
mod prompts {
    /// Title generation agent system prompt template
    pub const TITLE_GENERATOR: &str = "{{> system-prompt-title-generator.hbs }}";

    /// Help agent system prompt template
    pub const HELP: &str = "{{> system-prompt-help.hbs }}";

    /// Software engineer agent system prompt template
    pub const ENGINEER: &str = "{{> system-prompt-engineer.hbs }}";
}

/// Creates the default workflow using Rust constructors and setters
pub fn create_default_workflow() -> Workflow {
    // Define model IDs for reuse
    let advanced_model = ModelId::new("anthropic/claude-3.7-sonnet");
    let efficiency_model = ModelId::new("anthropic/claude-3.5-haiku");

    // Create the title generation worker agent
    let title_generation_worker = Agent::new(AgentId::new("title_generation_worker"))
        .model(efficiency_model.clone())
        .tool_supported(true)
        .tools(vec![ToolName::new("tool_forge_event_dispatch")])
        .subscribe(vec!["user_task_init".to_string()])
        .system_prompt(Template::<SystemContext>::new(prompts::TITLE_GENERATOR))
        .user_prompt(Template::<EventContext>::new(
            "<technical_content>{{event.value}}</technical_content>",
        ));

    // Create the help agent
    let help_agent = Agent::new(AgentId::new("help_agent"))
        .model(ModelId::new("google/gemini-2.0-flash-thinking-exp:free"))
        .tools(vec![
            ToolName::new("tool_forge_fs_read"),
            ToolName::new("tool_forge_fs_create"),
        ])
        .subscribe(vec!["user_help_query".to_string()])
        .system_prompt(Template::<SystemContext>::new(prompts::HELP))
        .user_prompt(Template::<EventContext>::new(
            "<query>{{event.value}}</query>",
        ));

    // Create the software engineer agent
    let software_engineer = Agent::new(AgentId::new("software-engineer"))
        .model(advanced_model)
        .tool_supported(true)
        .tools(vec![
            ToolName::new("tool_forge_fs_read"),
            ToolName::new("tool_forge_fs_create"),
            ToolName::new("tool_forge_fs_remove"),
            ToolName::new("tool_forge_fs_patch"),
            ToolName::new("tool_forge_process_shell"),
            ToolName::new("tool_forge_net_fetch"),
            ToolName::new("tool_forge_fs_search"),
        ])
        .subscribe(vec![
            "user_task_init".to_string(),
            "user_task_update".to_string(),
        ])
        .ephemeral(false)
        .max_walker_depth(4_usize)
        .system_prompt(Template::<SystemContext>::new(prompts::ENGINEER))
        .user_prompt(Template::<EventContext>::new(
            "<task>{{event.value}}</task>\n<mode>{{variables.mode}}</mode>",
        ));

    // Create variables map
    let mut variables = HashMap::new();
    variables.insert("mode".to_string(), json!("ACT"));

    // Create the workflow with all agents
    Workflow::default()
        .agents(vec![title_generation_worker, help_agent, software_engineer])
        .variables(variables)
}
