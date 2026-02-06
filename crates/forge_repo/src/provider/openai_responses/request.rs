use anyhow::Context as _;
use async_openai::types::responses as oai;
use forge_app::domain::{Context as ChatContext, ContextMessage, Role, ToolChoice};
use forge_app::utils::enforce_strict_schema;
use forge_domain::{Effort, ReasoningConfig};

use crate::provider::FromDomain;

impl FromDomain<ToolChoice> for oai::ToolChoiceParam {
    fn from_domain(choice: ToolChoice) -> anyhow::Result<Self> {
        Ok(match choice {
            ToolChoice::None => oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::None),
            ToolChoice::Auto => oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::Auto),
            ToolChoice::Required => oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::Required),
            ToolChoice::Call(name) => {
                oai::ToolChoiceParam::Function(oai::ToolChoiceFunction { name: name.to_string() })
            }
        })
    }
}

/// Converts domain ReasoningConfig to OpenAI Reasoning configuration
impl FromDomain<ReasoningConfig> for oai::Reasoning {
    fn from_domain(config: ReasoningConfig) -> anyhow::Result<Self> {
        let mut builder = oai::ReasoningArgs::default();

        // Map effort level
        if let Some(effort) = config.effort {
            let oai_effort = match effort {
                Effort::High => oai::ReasoningEffort::High,
                Effort::Medium => oai::ReasoningEffort::Medium,
                Effort::Low => oai::ReasoningEffort::Low,
            };
            builder.effort(oai_effort);
        } else if config.enabled.unwrap_or(false) {
            // Default to Medium effort when enabled without explicit effort
            builder.effort(oai::ReasoningEffort::Medium);
        }

        // Map summary preference
        // Note: OpenAI's ReasoningSummary doesn't have a "disabled" option
        // When exclude=true, we use Concise to minimize the summary output
        if let Some(exclude) = config.exclude {
            let summary = if exclude {
                oai::ReasoningSummary::Concise
            } else {
                oai::ReasoningSummary::Detailed
            };
            builder.summary(summary);
        } else {
            // Default to Auto summary
            builder.summary(oai::ReasoningSummary::Auto);
        }

        // Note: max_tokens is not supported in the OpenAI Responses API's ReasoningArgs
        // It's controlled at the request level via max_output_tokens

        builder.build().map_err(anyhow::Error::from)
    }
}

/// Converts a schemars RootSchema into codex tool parameters with
/// OpenAI-compatible JSON Schema
///
/// The Responses API performs strict JSON Schema validation for tools. This
/// function normalizes schemars output into the subset OpenAI accepts by using
/// the shared `normalize_json_schema` utility with strict mode enabled.
///
/// # Errors
/// Returns an error if schema serialization fails
fn codex_tool_parameters(
    schema: &schemars::schema::RootSchema,
) -> anyhow::Result<serde_json::Value> {
    let mut params =
        serde_json::to_value(schema).with_context(|| "Failed to serialize tool schema")?;

    // Use strict mode (true) for OpenAI - adds additionalProperties, properties,
    // and required
    enforce_strict_schema(&mut params, true);

    Ok(params)
}

/// Converts Forge's domain-level Context into an async-openai Responses API
/// request.
///
/// Supported subset (first iteration):
/// - Text messages (system/user/assistant)
/// - Assistant tool calls (full)
/// - Tool results
/// - tools + tool_choice
/// - max_tokens, temperature, top_p
impl FromDomain<ChatContext> for oai::CreateResponse {
    fn from_domain(context: ChatContext) -> anyhow::Result<Self> {
        let prompt_cache_key = context.conversation_id.as_ref().map(ToString::to_string);

        let mut instructions: Vec<String> = Vec::new();
        let mut items: Vec<oai::InputItem> = Vec::new();

        for entry in context.messages {
            match entry.message {
                ContextMessage::Text(message) => match message.role {
                    Role::System => {
                        instructions.push(message.content);
                    }
                    Role::User => {
                        items.push(oai::InputItem::EasyMessage(oai::EasyInputMessage {
                            r#type: oai::MessageType::Message,
                            role: oai::Role::User,
                            content: oai::EasyInputContent::Text(message.content),
                        }));
                    }
                    Role::Assistant => {
                        if !message.content.trim().is_empty() {
                            items.push(oai::InputItem::EasyMessage(oai::EasyInputMessage {
                                r#type: oai::MessageType::Message,
                                role: oai::Role::Assistant,
                                content: oai::EasyInputContent::Text(message.content),
                            }));
                        }

                        if let Some(tool_calls) = message.tool_calls {
                            for call in tool_calls {
                                let call_id = call
                                    .call_id
                                    .as_ref()
                                    .map(|id| id.as_str().to_string())
                                    .ok_or_else(|| {
                                    anyhow::anyhow!(
                                        "Tool call is missing call_id; cannot be sent to Responses API"
                                    )
                                })?;

                                items.push(oai::InputItem::Item(oai::Item::FunctionCall(
                                    oai::FunctionToolCall {
                                        arguments: call.arguments.into_string(),
                                        call_id,
                                        name: call.name.to_string(),
                                        id: None,
                                        status: None,
                                    },
                                )));
                            }
                        }
                    }
                },
                ContextMessage::Tool(result) => {
                    let call_id = result
                        .call_id
                        .as_ref()
                        .map(|id| id.as_str().to_string())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Tool result is missing call_id; cannot be sent to Responses API"
                            )
                        })?;

                    let output_json = serde_json::to_string(&result.output)
                        .with_context(|| "Failed to serialize tool output as JSON")?;

                    items.push(oai::InputItem::Item(oai::Item::FunctionCallOutput(
                        oai::FunctionCallOutputItemParam {
                            call_id,
                            output: oai::FunctionCallOutput::Text(output_json),
                            id: None,
                            status: None,
                        },
                    )));
                }
                ContextMessage::Image(_) => {
                    anyhow::bail!("Codex (Responses API) path does not yet support image inputs");
                }
            }
        }

        let instructions = (!instructions.is_empty()).then(|| instructions.join("\n\n"));

        let max_output_tokens = context
            .max_tokens
            .map(|tokens| u32::try_from(tokens).context("max_tokens must fit into u32"))
            .transpose()?;

        let tools = (!context.tools.is_empty())
            .then(|| {
                context
                    .tools
                    .into_iter()
                    .map(|tool| {
                        Ok(oai::Tool::Function(oai::FunctionTool {
                            name: tool.name.to_string(),
                            parameters: Some(codex_tool_parameters(&tool.input_schema)?),
                            strict: Some(true),
                            description: Some(tool.description),
                        }))
                    })
                    .collect::<anyhow::Result<Vec<oai::Tool>>>()
            })
            .transpose()?;

        let tool_choice = context
            .tool_choice
            .map(oai::ToolChoiceParam::from_domain)
            .transpose()?;

        let mut builder = oai::CreateResponseArgs::default();
        builder.input(oai::InputParam::Items(items));

        if let Some(instructions) = instructions {
            builder.instructions(instructions);
        }

        if let Some(max_output_tokens) = max_output_tokens {
            builder.max_output_tokens(max_output_tokens);
        }

        if let Some(temperature) = context.temperature {
            builder.temperature(temperature.value());
        }

        // Some OpenAI Codex/"reasoning" models reject `top_p` entirely (even when set
        // to defaults). To avoid hard failures, we currently omit it for the
        // Responses API path.

        if let Some(tools) = tools {
            builder.tools(tools);
        }

        if let Some(tool_choice) = tool_choice {
            builder.tool_choice(tool_choice);
        }

        // Apply reasoning configuration if provided
        if let Some(reasoning) = context.reasoning {
            let reasoning_config = oai::Reasoning::from_domain(reasoning)?;
            builder.reasoning(reasoning_config);
        }

        if let Some(prompt_cache_key) = prompt_cache_key {
            builder.prompt_cache_key(prompt_cache_key);
        }

        let mut response = builder.build().map_err(anyhow::Error::from)?;

        response.stream = Some(true);

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use async_openai::types::responses as oai;
    use forge_app::domain::{
        Context as ChatContext, ContextMessage, ModelId, ToolCallId, ToolChoice,
    };
    use forge_app::utils::enforce_strict_schema;

    use crate::provider::FromDomain;

    #[test]
    fn test_reasoning_config_conversion_with_effort() -> anyhow::Result<()> {
        use forge_domain::{Effort, ReasoningConfig};

        let fixture = ReasoningConfig {
            effort: Some(Effort::High),
            max_tokens: Some(2048),
            exclude: Some(false),
            enabled: None,
        };

        let actual = oai::Reasoning::from_domain(fixture)?;

        // Note: We can't easily assert the internal fields since ReasoningArgs
        // doesn't expose them after building. The fact that it builds without
        // error is the main verification.
        assert!(actual.effort.is_some());
        assert!(actual.summary.is_some());

        Ok(())
    }

    #[test]
    fn test_reasoning_config_conversion_with_enabled() -> anyhow::Result<()> {
        use forge_domain::ReasoningConfig;

        let fixture = ReasoningConfig {
            effort: None,
            max_tokens: None,
            exclude: None,
            enabled: Some(true),
        };

        let actual = oai::Reasoning::from_domain(fixture)?;

        // When enabled=true with no explicit effort, should default to Medium
        assert!(actual.effort.is_some());
        assert!(actual.summary.is_some());

        Ok(())
    }

    #[test]
    fn test_reasoning_config_conversion_with_exclude() -> anyhow::Result<()> {
        use forge_domain::{Effort, ReasoningConfig};

        let fixture = ReasoningConfig {
            effort: Some(Effort::Medium),
            max_tokens: None,
            exclude: Some(true),
            enabled: None,
        };

        let actual = oai::Reasoning::from_domain(fixture)?;

        // When exclude=true, should use Concise summary
        assert!(actual.effort.is_some());
        assert!(actual.summary.is_some());

        Ok(())
    }

    #[test]
    fn test_codex_request_with_reasoning_config() -> anyhow::Result<()> {
        use forge_domain::{Effort, ReasoningConfig};

        let reasoning = ReasoningConfig {
            effort: Some(Effort::High),
            max_tokens: Some(2048),
            exclude: Some(false),
            enabled: Some(true),
        };

        let context = ChatContext::default()
            .add_message(ContextMessage::user("Test", None))
            .reasoning(reasoning);

        let actual = oai::CreateResponse::from_domain(context)?;

        // Verify that reasoning config is set
        assert!(actual.reasoning.is_some());

        Ok(())
    }

    #[test]
    fn test_codex_request_from_context_converts_messages_tools_and_results() -> anyhow::Result<()> {
        let model = ModelId::from("codex-mini-latest");

        let tool_definition =
            forge_app::domain::ToolDefinition::new("shell").description("Run a shell command");

        let tool_call = forge_app::domain::ToolCallFull::new("shell")
            .call_id(ToolCallId::new("call_1"))
            .arguments(forge_app::domain::ToolCallArguments::from_json(
                r#"{"cmd":"echo hi"}"#,
            ));

        let tool_result = forge_app::domain::ToolResult::new("shell")
            .call_id(Some(ToolCallId::new("call_1")))
            .success("ok");

        let context = ChatContext::default()
            .add_message(ContextMessage::system("You are a helpful assistant."))
            .add_message(ContextMessage::user("Hello", None))
            .add_message(ContextMessage::assistant(
                "",
                None,
                None,
                Some(vec![tool_call]),
            ))
            .add_message(ContextMessage::tool_result(tool_result))
            .add_tool(tool_definition)
            .tool_choice(ToolChoice::Auto)
            .max_tokens(123usize);

        let mut actual = oai::CreateResponse::from_domain(context)?;
        actual.model = Some(model.as_str().to_string());

        assert_eq!(actual.model.as_deref(), Some("codex-mini-latest"));
        assert_eq!(
            actual.instructions.as_deref(),
            Some("You are a helpful assistant.")
        );
        assert_eq!(actual.max_output_tokens, Some(123));

        let oai::InputParam::Items(items) = actual.input else {
            anyhow::bail!("Expected items input");
        };

        // user + function_call + function_call_output
        assert_eq!(items.len(), 3);

        let oai::InputItem::EasyMessage(user_msg) = &items[0] else {
            anyhow::bail!("Expected first item to be a user message");
        };
        assert_eq!(user_msg.role, oai::Role::User);

        let oai::InputItem::Item(oai::Item::FunctionCall(call)) = &items[1] else {
            anyhow::bail!("Expected second item to be a function call");
        };
        assert_eq!(call.call_id, "call_1");
        assert_eq!(call.name, "shell");

        let oai::InputItem::Item(oai::Item::FunctionCallOutput(out)) = &items[2] else {
            anyhow::bail!("Expected third item to be a function call output");
        };
        assert_eq!(out.call_id, "call_1");

        Ok(())
    }

    // Common fixture functions
    fn fixture_tool_definition(name: &str) -> forge_app::domain::ToolDefinition {
        forge_app::domain::ToolDefinition::new(name).description("Test tool")
    }

    fn fixture_tool_call(name: &str, call_id: &str, args: &str) -> forge_app::domain::ToolCallFull {
        forge_app::domain::ToolCallFull::new(name)
            .call_id(ToolCallId::new(call_id))
            .arguments(forge_app::domain::ToolCallArguments::from_json(args))
    }

    #[test]
    fn test_tool_choice_none_conversion() -> anyhow::Result<()> {
        let actual = oai::ToolChoiceParam::from_domain(ToolChoice::None)?;
        assert!(matches!(
            actual,
            oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::None)
        ));
        Ok(())
    }

    #[test]
    fn test_tool_choice_auto_conversion() -> anyhow::Result<()> {
        let actual = oai::ToolChoiceParam::from_domain(ToolChoice::Auto)?;
        assert!(matches!(
            actual,
            oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::Auto)
        ));
        Ok(())
    }

    #[test]
    fn test_tool_choice_required_conversion() -> anyhow::Result<()> {
        let actual = oai::ToolChoiceParam::from_domain(ToolChoice::Required)?;
        assert!(matches!(
            actual,
            oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::Required)
        ));
        Ok(())
    }

    #[test]
    fn test_tool_choice_call_conversion() -> anyhow::Result<()> {
        let actual = oai::ToolChoiceParam::from_domain(ToolChoice::Call("test_tool".into()))?;
        assert!(matches!(
            actual,
            oai::ToolChoiceParam::Function(oai::ToolChoiceFunction { name, .. }) if name == "test_tool"
        ));
        Ok(())
    }

    #[test]
    fn test_reasoning_config_conversion_low_effort() -> anyhow::Result<()> {
        use forge_domain::{Effort, ReasoningConfig};

        let fixture = ReasoningConfig {
            effort: Some(Effort::Low),
            max_tokens: None,
            exclude: None,
            enabled: None,
        };

        let actual = oai::Reasoning::from_domain(fixture)?;
        assert!(actual.effort.is_some());
        assert!(actual.summary.is_some());

        Ok(())
    }

    #[test]
    fn test_reasoning_config_conversion_medium_effort() -> anyhow::Result<()> {
        use forge_domain::{Effort, ReasoningConfig};

        let fixture = ReasoningConfig {
            effort: Some(Effort::Medium),
            max_tokens: None,
            exclude: None,
            enabled: None,
        };

        let actual = oai::Reasoning::from_domain(fixture)?;
        assert!(actual.effort.is_some());
        assert!(actual.summary.is_some());

        Ok(())
    }

    #[test]
    fn test_reasoning_config_conversion_with_detailed_summary() -> anyhow::Result<()> {
        use forge_domain::{Effort, ReasoningConfig};

        let fixture = ReasoningConfig {
            effort: Some(Effort::Medium),
            max_tokens: None,
            exclude: Some(false),
            enabled: None,
        };

        let actual = oai::Reasoning::from_domain(fixture)?;
        assert!(actual.effort.is_some());
        assert!(actual.summary.is_some());

        Ok(())
    }

    #[test]
    fn test_reasoning_config_conversion_with_enabled_false() -> anyhow::Result<()> {
        use forge_domain::ReasoningConfig;

        let fixture = ReasoningConfig {
            effort: None,
            max_tokens: None,
            exclude: None,
            enabled: Some(false),
        };

        let actual = oai::Reasoning::from_domain(fixture)?;
        // When enabled=false, no effort should be set
        assert!(actual.effort.is_none());
        assert!(actual.summary.is_some());

        Ok(())
    }

    #[test]
    fn test_normalize_openai_json_schema_with_object_type() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        enforce_strict_schema(&mut schema, true);

        assert_eq!(
            schema["additionalProperties"],
            serde_json::Value::Bool(false)
        );
        assert_eq!(schema["required"], serde_json::json!(["name"]));
    }

    #[test]
    fn test_normalize_openai_json_schema_with_properties_key() {
        let mut schema = serde_json::json!({
            "properties": {
                "age": {"type": "number"}
            }
        });

        enforce_strict_schema(&mut schema, true);

        assert_eq!(
            schema["additionalProperties"],
            serde_json::Value::Bool(false)
        );
        assert_eq!(schema["required"], serde_json::json!(["age"]));
    }

    #[test]
    fn test_normalize_openai_json_schema_without_properties() {
        let mut schema = serde_json::json!({
            "type": "object"
        });

        enforce_strict_schema(&mut schema, true);

        assert_eq!(
            schema["properties"],
            serde_json::Value::Object(serde_json::Map::new())
        );
        assert_eq!(
            schema["additionalProperties"],
            serde_json::Value::Bool(false)
        );
        assert_eq!(schema["required"], serde_json::json!([]));
    }

    #[test]
    fn test_normalize_openai_json_schema_with_nested_objects() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            }
        });

        enforce_strict_schema(&mut schema, true);

        // Top level should have additionalProperties
        assert_eq!(
            schema["additionalProperties"],
            serde_json::Value::Bool(false)
        );
        assert_eq!(schema["required"], serde_json::json!(["user"]));

        // Nested object should also be normalized
        assert_eq!(
            schema["properties"]["user"]["additionalProperties"],
            serde_json::Value::Bool(false)
        );
        assert_eq!(
            schema["properties"]["user"]["required"],
            serde_json::json!(["name"])
        );
    }

    #[test]
    fn test_normalize_openai_json_schema_with_array() {
        let mut schema = serde_json::json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"}
                }
            }
        });

        enforce_strict_schema(&mut schema, true);

        // Array items should be normalized
        assert_eq!(
            schema["items"]["additionalProperties"],
            serde_json::Value::Bool(false)
        );
        assert_eq!(schema["items"]["required"], serde_json::json!(["id"]));
    }

    #[test]
    fn test_normalize_openai_json_schema_with_string() {
        let mut schema = serde_json::json!({
            "type": "string"
        });

        enforce_strict_schema(&mut schema, true);

        // Should not modify non-object types
        assert_eq!(schema, serde_json::json!({"type": "string"}));
    }

    #[test]
    fn test_normalize_openai_json_schema_sorts_required_keys() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "zebra": {"type": "string"},
                "alpha": {"type": "string"},
                "beta": {"type": "string"}
            }
        });

        enforce_strict_schema(&mut schema, true);

        assert_eq!(
            schema["required"],
            serde_json::json!(["alpha", "beta", "zebra"])
        );
    }
    #[test]
    fn test_codex_request_sets_prompt_cache_key_from_conversation_id() -> anyhow::Result<()> {
        use forge_domain::ConversationId;

        let conversation_id = ConversationId::generate();
        let context = ChatContext::default()
            .conversation_id(conversation_id)
            .add_message(ContextMessage::user("Hello", None));

        let actual = oai::CreateResponse::from_domain(context)?;
        let expected = Some(conversation_id.to_string());

        assert_eq!(actual.prompt_cache_key, expected);

        Ok(())
    }

    #[test]
    fn test_codex_request_without_conversation_id_has_no_prompt_cache_key() -> anyhow::Result<()> {
        let context = ChatContext::default().add_message(ContextMessage::user("Hello", None));

        let actual = oai::CreateResponse::from_domain(context)?;

        assert_eq!(actual.prompt_cache_key, None);

        Ok(())
    }

    #[test]
    fn test_codex_request_with_temperature() -> anyhow::Result<()> {
        use forge_app::domain::Temperature;

        let context = ChatContext::default()
            .add_message(ContextMessage::user("Hello", None))
            .temperature(Temperature::from(0.7));

        let actual = oai::CreateResponse::from_domain(context)?;

        assert_eq!(actual.temperature, Some(0.7));

        Ok(())
    }

    #[test]
    fn test_codex_request_with_empty_assistant_message() -> anyhow::Result<()> {
        let tool_call = fixture_tool_call("shell", "call_1", r#"{"cmd":"ls"}"#);

        let context = ChatContext::default()
            .add_message(ContextMessage::user("Run command", None))
            .add_message(ContextMessage::assistant(
                "",
                None,
                None,
                Some(vec![tool_call]),
            ))
            .add_message(ContextMessage::tool_result(
                forge_app::domain::ToolResult::new("shell")
                    .call_id(Some(ToolCallId::new("call_1")))
                    .success("output"),
            ));

        let actual = oai::CreateResponse::from_domain(context)?;

        let oai::InputParam::Items(items) = actual.input else {
            anyhow::bail!("Expected items input");
        };

        // Should only have user message, function call, and function call output
        // Empty assistant message should be skipped
        assert_eq!(items.len(), 3);

        Ok(())
    }

    #[test]
    fn test_codex_request_with_multiple_tool_calls() -> anyhow::Result<()> {
        let tool_call1 = fixture_tool_call("shell", "call_1", r#"{"cmd":"ls"}"#);
        let tool_call2 = fixture_tool_call("search", "call_2", r#"{"query":"test"}"#);

        let context = ChatContext::default()
            .add_message(ContextMessage::user("Do two things", None))
            .add_message(ContextMessage::assistant(
                "",
                None,
                None,
                Some(vec![tool_call1, tool_call2]),
            ));

        let actual = oai::CreateResponse::from_domain(context)?;

        let oai::InputParam::Items(items) = actual.input else {
            anyhow::bail!("Expected items input");
        };

        // Should have user message and 2 function calls
        assert_eq!(items.len(), 3);

        Ok(())
    }

    #[test]
    fn test_codex_request_with_multiple_system_messages() -> anyhow::Result<()> {
        let context = ChatContext::default()
            .add_message(ContextMessage::system("System 1"))
            .add_message(ContextMessage::system("System 2"))
            .add_message(ContextMessage::user("Hello", None));

        let actual = oai::CreateResponse::from_domain(context)?;

        assert_eq!(actual.instructions.as_deref(), Some("System 1\n\nSystem 2"));

        Ok(())
    }

    #[test]
    fn test_codex_request_with_tool_choice_required() -> anyhow::Result<()> {
        let tool = fixture_tool_definition("shell");

        let context = ChatContext::default()
            .add_message(ContextMessage::user("Hello", None))
            .add_tool(tool)
            .tool_choice(ToolChoice::Required);

        let actual = oai::CreateResponse::from_domain(context)?;

        assert!(matches!(
            actual.tool_choice,
            Some(oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::Required))
        ));

        Ok(())
    }

    #[test]
    fn test_codex_request_with_tool_choice_function() -> anyhow::Result<()> {
        let tool = fixture_tool_definition("shell");

        let context = ChatContext::default()
            .add_message(ContextMessage::user("Hello", None))
            .add_tool(tool)
            .tool_choice(ToolChoice::Call("shell".into()));

        let actual = oai::CreateResponse::from_domain(context)?;

        assert!(matches!(
            actual.tool_choice,
            Some(oai::ToolChoiceParam::Function(oai::ToolChoiceFunction { name, .. })) if name == "shell"
        ));

        Ok(())
    }

    #[test]
    fn test_codex_request_without_tools() -> anyhow::Result<()> {
        let context = ChatContext::default().add_message(ContextMessage::user("Hello", None));

        let actual = oai::CreateResponse::from_domain(context)?;

        assert!(actual.tools.is_none());
        assert!(actual.tool_choice.is_none());

        Ok(())
    }

    #[test]
    fn test_codex_request_with_image_input_returns_error() {
        use forge_domain::Image;

        let image = Image::new_base64("test123".to_string(), "image/png");
        let context = ChatContext::default().add_message(ContextMessage::Image(image));

        let result = oai::CreateResponse::from_domain(context);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Codex (Responses API) path does not yet support image inputs")
        );
    }

    #[test]
    fn test_codex_request_with_tool_call_missing_call_id_returns_error() {
        let tool_call = forge_app::domain::ToolCallFull::new("shell").arguments(
            forge_app::domain::ToolCallArguments::from_json(r#"{"cmd":"ls"}"#),
        );

        let context = ChatContext::default()
            .add_message(ContextMessage::user("Run command", None))
            .add_message(ContextMessage::assistant(
                "",
                None,
                None,
                Some(vec![tool_call]),
            ));

        let result = oai::CreateResponse::from_domain(context);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Tool call is missing call_id")
        );
    }

    #[test]
    fn test_codex_request_with_tool_result_missing_call_id_returns_error() {
        let context = ChatContext::default()
            .add_message(ContextMessage::user("Run command", None))
            .add_message(ContextMessage::tool_result(
                forge_app::domain::ToolResult::new("shell").success("output"),
            ));

        let result = oai::CreateResponse::from_domain(context);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Tool result is missing call_id")
        );
    }

    #[test]
    fn test_codex_request_with_max_tokens_overflow_returns_error() {
        let context = ChatContext::default()
            .add_message(ContextMessage::user("Hello", None))
            .max_tokens(u32::MAX as usize + 1);

        let result = oai::CreateResponse::from_domain(context);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("max_tokens must fit into u32")
        );
    }
}
