# Dynamic System Context Rendering with Variables

## Objective

Modify the system context rendering mechanism to:


1. Re-render the system context on every conversation turn
2. Pass variables to the system context renderer (similar to event rendering)

The current implementation renders the system context only once during agent initialization, which means time-sensitive information like `current_time` is not updated. Additionally, the system context cannot use dynamic variables from the conversation state.

## Implementation Plan

### 1. Update TemplateService Trait

Modify the `TemplateService` trait in `crates/forge_domain/src/services.rs` to include variables in the `render_system` method signature:

```rust
async fn render_system(
    &self,
    agent: &Agent,
    prompt: &Template<SystemContext>,
    variables: &HashMap<String, Value>,
) -> anyhow::Result<String>;
```

### 2. Modify ForgeTemplateService Implementation

Update the implementation in `crates/forge_services/src/template.rs` to handle variables in system context rendering:

```rust
async fn render_system(
    &self,
    _agent: &Agent,
    prompt: &Template<SystemContext>,
    variables: &HashMap<String, Value>,
) -> anyhow::Result<String> {
    // Existing code...
    
    // Create the context with README content for all agents
    let mut ctx = SystemContext {
        current_time,
        env: Some(env),
        tool_information: Some(self.tool_service.usage_prompt()),
        tool_supported: _agent.tool_supported.unwrap_or_default(),
        files,
        readme: README_CONTENT.to_string(),
        custom_rules: _agent.custom_rules.as_ref().cloned().unwrap_or_default(),
        variables: variables.clone(), // Add the variables
    };
    
    // Render the template with the context and variables
    let result = self.hb.render_template(prompt.template.as_str(), &ctx)?;
    Ok(result)
}
```

### 3. Update SystemContext Struct

Modify the `SystemContext` struct in `crates/forge_domain/src/system_context.rs` to include variables:

```rust
#[derive(Debug, Setters, Clone, Serialize, Deserialize)]
#[setters(strip_option)]
pub struct SystemContext {
    // Existing fields...
    
    // Variables passed to the template
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub variables: HashMap<String, Value>,
}
```

### 4. Update Orchestrator to Re-render System Context

Modify the orchestrator's conversation loop in `crates/forge_domain/src/orch.rs` to re-render the system context on each iteration:

```rust
async fn init_agent(&self, agent_id: &AgentId, event: &Event) -> anyhow::Result<()> {
    // Existing code...
    
    loop {
        // Get up-to-date variables
        let conversation_vars = self.conversation.read().await.variables.clone();
        
        // Re-render the system message if system_prompt is present
        if let Some(system_prompt) = &agent.system_prompt {
            let system_message = self
                .services
                .template_service()
                .render_system(agent, system_prompt, &conversation_vars)
                .await?;

            // Update the system message in the context
            context = context.set_first_system_message(system_message);
        }
        
        // Set context for the current loop iteration
        self.set_context(&agent.id, context.clone()).await?;
        
        // Existing loop code...
        
        // If no tool results, break the loop as before
        if tool_results.is_empty() {
            break;
        }
    }
    
    // Existing code...
}
```

### 5. Update System Context Templates

Ensure the templates can use variables from the context:

In `templates/forge-partial-system-info.hbs` and other system templates, add support for variables:

```handlebars
<system_info>
<operating_system>{{env.os}}</operating_system>
<current_time>{{current_time}}</current_time>
<current_working_directory>{{env.cwd}}</current_working_directory>
<default_shell>{{env.shell}}</default_shell>
<home_directory>{{env.home}}</home_directory>
<file_list>
{{#each files}} - {{this}}
{{/each}}
</file_list>
</system_info>

{{!-- Add access to variables --}}
{{#if variables}}
<variables>
{{#each variables}}
<{{@key}}>{{this}}</{{@key}}>
{{/each}}
</variables>
{{/if}}
```

### 6. Update Tests

Update relevant tests to verify the changes:


1. Add tests for the updated `render_system` method with variables
2. Ensure the system context is properly updated in the orchestrator tests
3. Verify that templates can correctly access variables in the system context

## Verification Criteria

The implementation will be considered successful if:


1. The system context is re-rendered on each conversation turn, with up-to-date `current_time`
2. Variables from the conversation state are correctly passed to the system context renderer
3. System context templates can access and display these variables
4. All tests pass with the new implementation
5. The overall API remains consistent with the existing paradigm

## Technical Design Notes


1. **Backward Compatibility**: The implementation should maintain backward compatibility with existing templates by making variables optional.
2. **Performance**: Re-rendering the system context on each turn is not expected to cause performance issues, as confirmed.
3. **Consistency**: The approach aligns with how event rendering already handles variables.
4. **Error Handling**: Proper error handling should be maintained throughout the implementation.


