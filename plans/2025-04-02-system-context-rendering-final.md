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
    let env = self.infra.environment_service().get_environment();

    // Build the walker, only setting max_depth if a value was provided
    let mut walker = Walker::max_all();

    // Only set max_depth if the value is provided
    // Create maximum depth for file walker, defaulting to 1 if not specified
    walker = walker.max_depth(_agent.max_walker_depth.unwrap_or(1));

    let mut files = walker
        .cwd(env.cwd.clone())
        .get()
        .await?
        .iter()
        .map(|f| f.path.to_string())
        .collect::<Vec<_>>();

    // Sort the files alphabetically to ensure consistent ordering
    files.sort();

    // Get current date and time with timezone
    let current_time = Local::now().format("%Y-%m-%d %H:%M:%S %:z").to_string();

    // Create the context with README content for all agents
    let ctx = SystemContext {
        current_time,
        env: Some(env),
        tool_information: Some(self.tool_service.usage_prompt()),
        tool_supported: _agent.tool_supported.unwrap_or_default(),
        files,
        readme: README_CONTENT.to_string(),
        custom_rules: _agent.custom_rules.as_ref().cloned().unwrap_or_default(),
        variables: variables.clone(), // Add the variables
    };

    // Render the template with the context
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
    // Current date and time at the time of context creation
    pub current_time: String,
    
    // Environment information to be included in the system context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Environment>,

    // Information about available tools that can be used by the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_information: Option<String>,

    /// Indicates whether the agent supports tools.
    /// This value is populated directly from the Agent configuration.
    #[serde(default)]
    pub tool_supported: bool,

    // List of file paths that are relevant for the agent context
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,

    // README content to provide project context to the agent
    pub readme: String,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub custom_rules: String,
    
    // Variables to pass to the system context
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variables: HashMap<String, Value>,
}

// Update the Default implementation if it exists
impl Default for SystemContext {
    fn default() -> Self {
        Self {
            current_time: String::new(),
            env: None,
            tool_information: None,
            tool_supported: false,
            files: Vec::new(),
            readme: String::new(),
            custom_rules: String::new(),
            variables: HashMap::new(),
        }
    }
}
```

### 4. Update Orchestrator to Re-render System Context

Modify the orchestrator's conversation loop in `crates/forge_domain/src/orch.rs` to re-render the system context on each iteration:

```rust
async fn init_agent(&self, agent_id: &AgentId, event: &Event) -> anyhow::Result<()> {
    let conversation = self.get_conversation().await?;
    debug!(
        conversation_id = %conversation.id,
        agent = %agent_id,
        event = ?event,
        "Initializing agent"
    );
    let agent = conversation.workflow.get_agent(agent_id)?;

    let mut context = if agent.ephemeral.unwrap_or_default() {
        self.init_agent_context(agent).await?
    } else {
        match conversation.context(&agent.id) {
            Some(context) => context.clone(),
            None => self.init_agent_context(agent).await?,
        }
    };

    if let Some(temperature) = agent.temperature {
        context = context.temperature(temperature);
    }

    let content = if let Some(user_prompt) = &agent.user_prompt {
        // Get conversation variables from the conversation
        let variables = &conversation.variables;

        // Use the consolidated render_event method which handles suggestions and
        // variables
        self.services
            .template_service()
            .render_event(agent, user_prompt, event, variables)
            .await?
    } else {
        // Use the raw event value as content if no user_prompt is provided
        event.value.to_string()
    };

    if !content.is_empty() {
        context = context.add_message(ContextMessage::user(content));
    }

    // Process attachments
    let attachments = self
        .services
        .attachment_service()
        .attachments(&event.value.to_string())
        .await?;

    for attachment in attachments.into_iter() {
        match attachment.content_type {
            ContentType::Image => {
                context = context.add_message(ContextMessage::Image(attachment.content));
            }
            ContentType::Text => {
                let content = format!(
                    "<file_content path=\"{}\">{}</file_content>",
                    attachment.path, attachment.content
                );
                context = context.add_message(ContextMessage::user(content));
            }
        }
    }

    self.set_context(&agent.id, context.clone()).await?;

    loop {
        // Get the latest conversation variables
        let variables = self.conversation.read().await.variables.clone();
        
        // Re-render system prompt if present
        if let Some(system_prompt) = &agent.system_prompt {
            let system_message = self
                .services
                .template_service()
                .render_system(agent, system_prompt, &variables)
                .await?;
                
            context = context.set_first_system_message(system_message);
        }
        
        // Set context for the current loop iteration
        self.set_context(&agent.id, context.clone()).await?;
        let response = self
            .services
            .provider_service()
            .chat(
                agent
                    .model
                    .as_ref()
                    .ok_or(Error::MissingModel(agent.id.clone()))?,
                context.clone(),
            )
            .await?;
        let ChatCompletionResult { tool_calls, content } =
            self.collect_messages(agent, response).await?;

        // Get all tool results using the helper function
        let tool_results = self.get_all_tool_results(agent, &tool_calls).await?;

        context = context
            .add_message(ContextMessage::assistant(content, Some(tool_calls)))
            .add_tool_results(tool_results.clone());

        // Check if context requires compression
        context = self.compactor.compact_context(agent, context).await?;

        self.set_context(&agent.id, context.clone()).await?;
        self.sync_conversation().await?;

        if tool_results.is_empty() {
            break;
        }
    }

    self.complete_turn(&agent.id).await?;

    self.sync_conversation().await?;

    Ok(())
}
```

### 5. Update Tests

Add tests for the updated `render_system` method with variables:

```rust
// In forge_services/src/template.rs or a test file
#[tokio::test]
async fn test_render_system_with_variables() {
    // Create a test agent
    let agent = Agent::new("test-agent")
        .system_prompt(Template::from_string("{{current_time}} - {{variables.test_var}}"));
    
    // Create test variables
    let mut variables = HashMap::new();
    variables.insert("test_var".to_string(), json!("test_value"));
    
    // Render the system prompt with variables
    let result = template_service
        .render_system(&agent, agent.system_prompt.as_ref().unwrap(), &variables)
        .await
        .unwrap();
    
    // Verify the result contains both the current time and the variable
    assert!(result.contains("test_value"));
}
```

### 6. Update init_agent_context Method

Also need to update the `init_agent_context` method in `crates/forge_domain/src/orch.rs` to pass empty variables:

```rust
async fn init_agent_context(&self, agent: &Agent) -> anyhow::Result<Context> {
    let tool_defs = self.init_tool_definitions(agent);

    // Use the agent's tool_supported flag directly instead of querying the provider
    let tool_supported = agent.tool_supported.unwrap_or_default();

    let mut context = Context::default();

    if let Some(system_prompt) = &agent.system_prompt {
        // Create empty variables for initial rendering
        let empty_variables = HashMap::new();
        
        let system_message = self
            .services
            .template_service()
            .render_system(agent, system_prompt, &empty_variables)
            .await?;

        context = context.set_first_system_message(system_message);
    }

    Ok(context.extend_tools(if tool_supported {
        tool_defs
    } else {
        Vec::new()
    }))
}
```

## Verification Criteria

The implementation will be considered successful if:


1. The system context is re-rendered on each conversation turn, ensuring up-to-date information
2. Variables from the conversation state are correctly passed to the system context renderer
3. System context templates can access and display these variables (templates have already been modified)
4. All tests pass with the new implementation
5. The code is clean, without unnecessary backward compatibility layers

## Technical Design Notes


1. **Clean Approach**: The implementation takes a clean approach without backward compatibility concerns, as specified.
2. **Performance**: Re-rendering the system context on each turn should not cause performance issues.
3. **Consistency**: The approach aligns with how event rendering already handles variables.
4. **Error Handling**: Proper error handling is maintained throughout the implementation.


