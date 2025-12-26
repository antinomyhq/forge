use anyhow::Context as _;
use async_openai::types::responses as oai;
use forge_app::domain::{Context as ChatContext, ContextMessage, Role, ToolChoice};

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

fn normalize_openai_json_schema(schema: &mut serde_json::Value) {
    match schema {
        serde_json::Value::Object(map) => {
            let is_object = map
                .get("type")
                .and_then(|value| value.as_str())
                .is_some_and(|ty| ty == "object")
                || map.contains_key("properties");

            if is_object {
                if !map.contains_key("properties") {
                    map.insert(
                        "properties".to_string(),
                        serde_json::Value::Object(serde_json::Map::new()),
                    );
                }

                // OpenAI requires this field to exist and be `false` for objects.
                map.insert(
                    "additionalProperties".to_string(),
                    serde_json::Value::Bool(false),
                );

                // OpenAI requires `required` to exist and include every property key.
                let required_keys = map
                    .get("properties")
                    .and_then(|value| value.as_object())
                    .map(|props| {
                        let mut keys = props.keys().cloned().collect::<Vec<_>>();
                        keys.sort();
                        keys
                    })
                    .unwrap_or_default();

                let required_values = required_keys
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect::<Vec<_>>();

                map.insert(
                    "required".to_string(),
                    serde_json::Value::Array(required_values),
                );
            }

            for value in map.values_mut() {
                normalize_openai_json_schema(value);
            }
        }
        serde_json::Value::Array(items) => {
            for value in items {
                normalize_openai_json_schema(value);
            }
        }
        _ => {}
    }
}

fn codex_tool_parameters(
    schema: &schemars::schema::RootSchema,
) -> anyhow::Result<serde_json::Value> {
    let mut params =
        serde_json::to_value(schema).with_context(|| "Failed to serialize tool schema")?;

    // The Responses API performs strict JSON Schema validation for tools; normalize
    // schemars output into the subset OpenAI accepts.
    normalize_openai_json_schema(&mut params);

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

        // Enable reasoning for o-series and gpt-5 models
        // This is required to receive reasoning text in the response
        let reasoning_config = oai::ReasoningArgs::default()
            .effort(oai::ReasoningEffort::Medium)
            .summary(oai::ReasoningSummary::Auto)
            .build()
            .map_err(anyhow::Error::from)?;
        builder.reasoning(reasoning_config);

        builder.build().map_err(anyhow::Error::from)
    }
}

#[cfg(test)]
mod tests {
    use async_openai::types::responses as oai;
    use forge_app::domain::{
        Context as ChatContext, ContextMessage, ModelId, ToolCallId, ToolChoice,
    };

    use crate::provider::FromDomain;

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
            .add_message(ContextMessage::assistant("", None, Some(vec![tool_call])))
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
}
