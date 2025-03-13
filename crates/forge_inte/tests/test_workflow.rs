use forge_domain::{
    Agent, AgentId, EventContext, ModelId, SystemContext, Template, ToolName, Workflow,
};

/// System prompt for the developer agent
const SYSTEM_PROMPT: &str = r#"
Use the tools at your disposal and solve the user given task.
First, let's establish the current system information:
<system_info>
<operating_system>{{env.os}}</operating_system>
<current_working_directory>{{env.cwd}}</current_working_directory>
<default_shell>{{env.shell}}</default_shell>
<home_directory>{{env.home}}</home_directory>
<file_list>
{{#each files}} - {{this}}
{{/each}}
</file_list>
</system_info>
"#;

/// User prompt for the developer agent
const USER_PROMPT: &str = r#"<task>{{event.value}}</task>

Hints:
- There is a .md file that contains the name of the cat.
"#;

/// Creates a test workflow that mimics the configuration from
/// test_workflow.yaml
pub fn create_test_workflow() -> Workflow {
    // Create the developer agent
    let developer = Agent::new(AgentId::new("developer"))
        .model(ModelId::new("anthropic/claude-3.5-sonnet"))
        .tool_supported(true)
        .tools(vec![
            ToolName::new("tool_forge_fs_read"),
            ToolName::new("tool_forge_fs_search"),
        ])
        .subscribe(vec!["user_task_init".to_string()])
        .ephemeral(false)
        .system_prompt(Template::<SystemContext>::new(SYSTEM_PROMPT.trim()))
        .user_prompt(Template::<EventContext>::new(USER_PROMPT.trim()));

    Workflow::default().agents(vec![developer])
}
