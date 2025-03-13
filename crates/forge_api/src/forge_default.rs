//! Default configuration for Forge
//!
//! This module contains the default configuration that is used when no
//! custom configuration is provided.

use std::collections::HashMap;

use forge_domain::{
    Agent, AgentId, EventContext, ModelId, SystemContext, Template, ToolName, Workflow,
};
use serde_json::json;

/// System prompt templates for each agent type
mod prompts {
    /// Title generation agent system prompt template
    pub const TITLE_GENERATOR: &str = "{{> system-prompt-title-generator.hbs }}";

    /// Help agent system prompt template
    pub const HELP: &str = "{{> system-prompt-help.hbs }}";

    /// Software engineer agent system prompt template
    pub const ENGINEER: &str = "{{> system-prompt-engineer.hbs }}";

    /// GitHub engineer agent system prompt template - extends the regular
    /// engineer
    pub const GITHUB_ENGINEER: &str = "{{> system-prompt-github-engineer.hbs }}";
}

/// Common tools used by engineer-type agents
fn common_engineer_tools() -> Vec<ToolName> {
    vec![
        ToolName::new("tool_forge_fs_read"),
        ToolName::new("tool_forge_fs_create"),
        ToolName::new("tool_forge_fs_remove"),
        ToolName::new("tool_forge_fs_patch"),
        ToolName::new("tool_forge_process_shell"),
        ToolName::new("tool_forge_net_fetch"),
        ToolName::new("tool_forge_fs_search"),
    ]
}

/// Helper function to configure common settings for engineer-type agents
fn configure_engineer_agent(
    agent_id: AgentId,
    model: ModelId,
    additional_tools: Vec<ToolName>,
) -> Agent {
    // Get the common tools
    let mut tools = common_engineer_tools();

    // Add any additional tools
    tools.extend(additional_tools);

    Agent::new(agent_id)
        .model(model)
        .tool_supported(true)
        .tools(tools)
        .ephemeral(false)
        .max_walker_depth(4_usize)
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

    // Create the software engineer agent using the common configuration
    let software_engineer = configure_engineer_agent(
        AgentId::new("software-engineer"),
        advanced_model.clone(),
        vec![], // No additional tools
    )
    .subscribe(vec![
        "user_task_init".to_string(),
        "user_task_update".to_string(),
    ])
    .system_prompt(Template::<SystemContext>::new(prompts::ENGINEER))
    .user_prompt(Template::<EventContext>::new(
        "<task>{{event.value}}</task>\n<mode>{{variables.mode}}</mode>",
    ));

    // Create variables map
    let mut variables = HashMap::new();
    variables.insert("mode".to_string(), json!("ACT"));

    // Create the GitHub task agent with additional tool
    let github_task_agent = configure_engineer_agent(
        AgentId::new("github-task-agent"),
        advanced_model.clone(),
        vec![ToolName::new("tool_forge_event_dispatch")], // GitHub-specific additional tool
    )
    .subscribe(vec!["fix_issue".to_string(), "update_pr".to_string()])
    .system_prompt(Template::<SystemContext>::new(prompts::GITHUB_ENGINEER))
    .user_prompt(Template::<EventContext>::new(
        "<event>{{event.name}}</event>\n<value>{{event.value}}</value>\n<mode>ACT</mode>",
    ));

    // Create the workflow with all agents
    Workflow::default()
        .agents(vec![
            title_generation_worker,
            help_agent,
            software_engineer,
            github_task_agent,
        ])
        .variables(variables)
}
