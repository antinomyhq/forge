#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use forge_api::API;
use forge_domain::{
    AgentId, Conversation, ConversationId, DataGenerationParameters, McpConfig, ProviderId, Scope,
    UserPrompt,
};
use futures::StreamExt;
use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::{RpcModule, SubscriptionMessage};
use serde_json::{Value, json};
use tracing::debug;

use crate::error::{ErrorCode, map_error, not_found};
use crate::transport::stdio::StdioTransport;
use crate::types::*;

/// Helper to serialize a response value, mapping errors to JSON-RPC error
fn to_json_response<T: serde::Serialize>(value: T) -> Result<Value, ErrorObjectOwned> {
    serde_json::to_value(value).map_err(|e| {
        ErrorObjectOwned::owned(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to serialize response: {}", e),
            None::<()>,
        )
    })
}

/// STDIO-based JSON-RPC server wrapping the Forge API
pub struct JsonRpcServer<A: API> {
    api: Arc<A>,
    module: RpcModule<()>,
}

impl<A: API + 'static> JsonRpcServer<A> {
    /// Create a new JSON-RPC server with the given API implementation
    pub fn new(api: Arc<A>) -> Self {
        let mut server = Self { api, module: RpcModule::new(()) };
        server.register_methods();
        server
    }

    /// Get a reference to the underlying API
    pub fn api(&self) -> &Arc<A> {
        &self.api
    }

    /// Build the RPC module with all method registrations
    fn register_methods(&mut self) {
        self.register_discovery_methods();
        self.register_conversation_methods();
        self.register_workspace_methods();
        self.register_config_methods();
        self.register_auth_methods();
        self.register_system_methods();
    }

    /// Register discovery methods (get_models, get_agents, get_tools, discover)
    fn register_discovery_methods(&mut self) {
        // get_models
        let api = self.api.clone();
        self.module
            .register_async_method("get_models", move |_, _, _| {
                let api = api.clone();
                async move {
                    let models = api.get_models().await.map_err(map_error)?;
                    let response: Vec<ModelResponse> =
                        models.into_iter().map(ModelResponse::from).collect();
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_models");

        // get_agents
        let api = self.api.clone();
        self.module
            .register_async_method("get_agents", move |_, _, _| {
                let api = api.clone();
                async move {
                    let agents = api.get_agents().await.map_err(map_error)?;
                    let response: Vec<AgentResponse> =
                        agents.into_iter().map(AgentResponse::from).collect();
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_agents");

        // get_tools
        let api = self.api.clone();
        self.module
            .register_async_method("get_tools", move |_, _, _| {
                let api = api.clone();
                async move {
                    let tools = api.get_tools().await.map_err(map_error)?;
                    let mut all_tools: Vec<String> = Vec::new();
                    // System tools - ToolName is a newtype, use .to_string() or .0
                    all_tools.extend(tools.system.iter().map(|t| t.name.to_string()));
                    // Agent tools
                    all_tools.extend(tools.agents.iter().map(|t| t.name.to_string()));
                    // MCP tools - iterate through the HashMap
                    for server_tools in tools.mcp.get_servers().values() {
                        all_tools.extend(server_tools.iter().map(|t| t.name.to_string()));
                    }
                    let response =
                        ToolsOverviewResponse { enabled: all_tools, disabled: Vec::new() };
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_tools");

        // discover
        let api = self.api.clone();
        self.module
            .register_async_method("discover", move |_, _, _| {
                let api = api.clone();
                async move {
                    let files = api.discover().await.map_err(map_error)?;
                    let response: Vec<FileResponse> =
                        files.into_iter().map(FileResponse::from).collect();
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register discover");

        // get_providers
        let api = self.api.clone();
        self.module
            .register_async_method("get_providers", move |_, _, _| {
                let api = api.clone();
                async move {
                    let providers = api.get_providers().await.map_err(map_error)?;
                    let response: Vec<ProviderResponse> =
                        providers.into_iter().map(ProviderResponse::from).collect();
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_providers");

        // get_all_provider_models
        let api = self.api.clone();
        self.module
            .register_async_method("get_all_provider_models", move |_, _, _| {
                let api = api.clone();
                async move {
                    let provider_models = api.get_all_provider_models().await.map_err(map_error)?;
                    let response: Vec<ProviderModelsResponse> = provider_models
                        .into_iter()
                        .map(ProviderModelsResponse::from)
                        .collect();
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_all_provider_models");

        // get_provider - Get a single provider by ID
        let api = self.api.clone();
        self.module
            .register_async_method("get_provider", move |params, _, _| {
                let api = api.clone();
                async move {
                    let provider_id_str: String = params.parse()?;
                    let provider_id = ProviderId::from_str(&provider_id_str).map_err(|_| {
                        ErrorObjectOwned::owned(-32602, "Invalid provider ID", None::<()>)
                    })?;

                    let provider = api.get_provider(&provider_id).await.map_err(map_error)?;

                    let response = ProviderResponse {
                        id: provider.id().to_string(),
                        name: provider.id().to_string(),
                        api_key: None,
                    };

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_provider");

        // get_agent_provider - Get provider for a specific agent
        let api = self.api.clone();
        self.module
            .register_async_method("get_agent_provider", move |params, _, _| {
                let api = api.clone();
                async move {
                    let agent_id_str: String = params.parse()?;
                    let agent_id = AgentId::new(&agent_id_str);

                    let provider = api.get_agent_provider(agent_id).await.map_err(map_error)?;

                    let response = ProviderResponse {
                        id: provider.id.to_string(),
                        name: provider.id.to_string(),
                        api_key: None,
                    };

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_agent_provider");

        // get_default_provider - Get the default provider
        let api = self.api.clone();
        self.module
            .register_async_method("get_default_provider", move |_, _, _| {
                let api = api.clone();
                async move {
                    let provider = api.get_default_provider().await.map_err(map_error)?;

                    let response = ProviderResponse {
                        id: provider.id.to_string(),
                        name: provider.id.to_string(),
                        api_key: None,
                    };

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_default_provider");

        // get_schema - returns the API schema for all methods
        self.module
            .register_method("get_schema", |_, _, _| {
                // Return the complete API schema
                let schema = get_api_schema();
                Ok::<_, ErrorObjectOwned>(schema)
            })
            .expect("Failed to register get_schema");

        // get_methods - returns a list of all available methods with their signatures
        self.module
            .register_method("get_methods", |_, _, _| {
                let methods = get_all_jsonrpc_methods();
                Ok::<_, ErrorObjectOwned>(json!({
                    "version": "1.0.0",
                    "methods": methods
                }))
            })
            .expect("Failed to register get_methods");

        // get_types - returns all type definitions
        self.module
            .register_method("get_types", |_, _, _| {
                let types = get_all_types();
                Ok::<_, ErrorObjectOwned>(json!({
                    "version": "1.0.0",
                    "types": types
                }))
            })
            .expect("Failed to register get_types");

        // rpc.discover - OpenRPC standard discovery method
        self.module
            .register_method("rpc.discover", |_, _, _| {
                let schema = get_openrpc_schema();
                Ok::<_, ErrorObjectOwned>(schema)
            })
            .expect("Failed to register rpc.discover");

        // rpc.methods - Standard JSON-RPC method enumeration
        self.module
            .register_method("rpc.methods", |_, _, _| {
                let methods = get_all_jsonrpc_methods();
                Ok::<_, ErrorObjectOwned>(json!({
                    "methods": methods.iter().map(|m| m["name"].as_str().unwrap_or("")).collect::<Vec<_>>()
                }))
            })
            .expect("Failed to register rpc.methods");

        // rpc.describe - Describe a specific method (standard introspection)
        self.module
            .register_method("rpc.describe", |params, _, _| {
                let method_name: String = params.parse()?;
                let all_methods = get_all_jsonrpc_methods();
                let method = all_methods
                    .iter()
                    .find(|m| m["name"].as_str() == Some(&method_name));

                match method {
                    Some(m) => Ok::<_, ErrorObjectOwned>(m.clone()),
                    None => Err(ErrorObjectOwned::owned(
                        -32602,
                        format!("Method '{}' not found", method_name),
                        None::<()>,
                    )),
                }
            })
            .expect("Failed to register rpc.describe");
    }

    /// Register conversation methods
    fn register_conversation_methods(&mut self) {
        let _api = self.api.clone();

        // get_conversations
        let api = self.api.clone();
        self.module
            .register_async_method("get_conversations", move |params, _, _| {
                let api = api.clone();
                async move {
                    let limit: Option<usize> = params.parse().ok();

                    let conversations = api.get_conversations(limit).await.map_err(map_error)?;
                    let response: Vec<ConversationResponse> = conversations
                        .into_iter()
                        .map(|c| ConversationResponse {
                            id: c.id.into_string(),
                            title: c.title,
                            created_at: c.metadata.created_at.to_rfc3339(),
                            updated_at: c.metadata.updated_at.map(|t| t.to_rfc3339()),
                            message_count: c.context.as_ref().map(|ctx| ctx.messages.len()),
                        })
                        .collect();
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_conversations");

        // conversation (get single conversation)
        let api = self.api.clone();
        self.module
            .register_async_method("conversation", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: ConversationParams = params.parse()?;
                    let conversation_id =
                        ConversationId::parse(&params.conversation_id).map_err(|e| {
                            ErrorObjectOwned::owned(
                                ErrorCode::INVALID_PARAMS,
                                format!("Invalid conversation_id: {}", e),
                                None::<()>,
                            )
                        })?;

                    let conversation = api
                        .conversation(&conversation_id)
                        .await
                        .map_err(map_error)?;

                    match conversation {
                        Some(c) => {
                            let response = ConversationResponse {
                                id: c.id.into_string(),
                                title: c.title,
                                created_at: c.metadata.created_at.to_rfc3339(),
                                updated_at: c.metadata.updated_at.map(|t| t.to_rfc3339()),
                                message_count: c.context.as_ref().map(|ctx| ctx.messages.len()),
                            };
                            Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                        }
                        None => Err(not_found("Conversation", &params.conversation_id)),
                    }
                }
            })
            .expect("Failed to register conversation");

        // upsert_conversation
        let api = self.api.clone();
        self.module
            .register_async_method("upsert_conversation", move |params, _, _| {
                let api = api.clone();
                async move {
                    // Parse the conversation from params before spawning
                    let conversation_json: Value = params.parse()?;
                    let conversation: Conversation = serde_json::from_value(conversation_json)
                        .map_err(|e| {
                            ErrorObjectOwned::owned(
                                ErrorCode::INVALID_PARAMS,
                                format!("Invalid conversation: {}", e),
                                None::<()>,
                            )
                        })?;

                    api.upsert_conversation(conversation)
                        .await
                        .map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register upsert_conversation");

        // delete_conversation
        let api = self.api.clone();
        self.module
            .register_async_method("delete_conversation", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: ConversationParams = params.parse()?;
                    let conversation_id =
                        ConversationId::parse(&params.conversation_id).map_err(|e| {
                            ErrorObjectOwned::owned(
                                ErrorCode::INVALID_PARAMS,
                                format!("Invalid conversation_id: {}", e),
                                None::<()>,
                            )
                        })?;

                    api.delete_conversation(&conversation_id)
                        .await
                        .map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register delete_conversation");

        // rename_conversation
        let api = self.api.clone();
        self.module
            .register_async_method("rename_conversation", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: RenameConversationParams = params.parse()?;
                    let conversation_id =
                        ConversationId::parse(&params.conversation_id).map_err(|e| {
                            ErrorObjectOwned::owned(
                                ErrorCode::INVALID_PARAMS,
                                format!("Invalid conversation_id: {}", e),
                                None::<()>,
                            )
                        })?;

                    api.rename_conversation(&conversation_id, params.title)
                        .await
                        .map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register rename_conversation");

        // last_conversation
        let api = self.api.clone();
        self.module
            .register_async_method("last_conversation", move |_, _, _| {
                let api = api.clone();
                async move {
                    let conversation = api.last_conversation().await.map_err(map_error)?;

                    let response = conversation.map(|c| ConversationResponse {
                        id: c.id.into_string(),
                        title: c.title,
                        created_at: c.metadata.created_at.to_rfc3339(),
                        updated_at: c.metadata.updated_at.map(|t| t.to_rfc3339()),
                        message_count: c.context.as_ref().map(|ctx| ctx.messages.len()),
                    });

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register last_conversation");

        // compact_conversation
        let api = self.api.clone();
        self.module
            .register_async_method("compact_conversation", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: CompactConversationParams = params.parse()?;
                    let conversation_id =
                        ConversationId::parse(&params.conversation_id).map_err(|e| {
                            ErrorObjectOwned::owned(
                                ErrorCode::INVALID_PARAMS,
                                format!("Invalid conversation_id: {}", e),
                                None::<()>,
                            )
                        })?;

                    let result = api
                        .compact_conversation(&conversation_id)
                        .await
                        .map_err(map_error)?;

                    let response = CompactionResultResponse {
                        original_tokens: result.original_tokens,
                        compacted_tokens: result.compacted_tokens,
                        original_messages: result.original_messages,
                        compacted_messages: result.compacted_messages,
                    };

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register compact_conversation");

        // chat.stream - subscription-based streaming for real-time updates
        let api = self.api.clone();
        self.module
            .register_subscription(
                "chat.stream",
                "chat.notification",
                "chat.stream.unsubscribe",
                move |params, pending, _, _| {
                    let api = api.clone();
                    async move {
                        use forge_domain::ChatRequest;

                        let params: ChatParams = params.parse()?;
                        let conversation_id = ConversationId::parse(&params.conversation_id)
                            .map_err(|e| {
                                ErrorObjectOwned::owned(
                                    ErrorCode::INVALID_PARAMS,
                                    format!("Invalid conversation_id: {}", e),
                                    None::<()>,
                                )
                            })?;

                        let event = forge_domain::Event::new(forge_domain::EventValue::text(
                            params.message,
                        ));
                        let chat_req = ChatRequest::new(event, conversation_id);

                        let stream = api.chat(chat_req).await.map_err(map_error)?;
                        let sink = pending.accept().await?;

                        tokio::spawn(async move {
                            let mut stream = stream;
                            while let Some(result) = stream.next().await {
                                let msg = match result {
                                    Ok(chat_msg) => {
                                        let data = match chat_msg {
                                            forge_domain::ChatResponse::TaskMessage {
                                                content,
                                                ..
                                            } => {
                                                json!({
                                                    "type": "message",
                                                    "content": content.as_str()
                                                })
                                            }
                                            forge_domain::ChatResponse::TaskReasoning {
                                                content,
                                            } => {
                                                json!({
                                                    "type": "reasoning",
                                                    "content": content
                                                })
                                            }
                                            forge_domain::ChatResponse::TaskComplete => {
                                                json!({
                                                    "type": "complete"
                                                })
                                            }
                                            forge_domain::ChatResponse::ToolCallStart {
                                                ..
                                            } => {
                                                json!({
                                                    "type": "tool_start"
                                                })
                                            }
                                            forge_domain::ChatResponse::ToolCallEnd(_) => {
                                                json!({
                                                    "type": "tool_end"
                                                })
                                            }
                                            forge_domain::ChatResponse::RetryAttempt {
                                                cause,
                                                ..
                                            } => {
                                                json!({
                                                    "type": "retry",
                                                    "cause": cause.as_str()
                                                })
                                            }
                                            forge_domain::ChatResponse::Interrupt { reason } => {
                                                json!({
                                                    "type": "interrupt",
                                                    "reason": format!("{:?}", reason)
                                                })
                                            }
                                        };
                                        StreamMessage::Chunk { data }
                                    }
                                    Err(e) => StreamMessage::Error { message: format!("{:#}", e) },
                                };

                                let sub_msg =
                                    SubscriptionMessage::from_json(&msg).unwrap_or_else(|_| {
                                        SubscriptionMessage::from_json(&json!({"status": "error"}))
                                            .expect("fallback message should never fail")
                                    });
                                if sink.send(sub_msg).await.is_err() {
                                    debug!("Client disconnected from chat stream");
                                    break;
                                }
                            }

                            let complete_msg =
                                SubscriptionMessage::from_json(&StreamMessage::Complete)
                                    .unwrap_or_else(|_| {
                                        SubscriptionMessage::from_json(
                                            &json!({"status": "complete"}),
                                        )
                                        .unwrap_or_else(
                                            |_| {
                                                SubscriptionMessage::from_json(
                                                    &json!({"done": true}),
                                                )
                                                .expect("fallback message should never fail")
                                            },
                                        )
                                    });
                            let _ = sink.send(complete_msg).await;
                        });

                        Ok(())
                    }
                },
            )
            .expect("Failed to register chat.stream");
    }

    /// Register workspace methods
    fn register_workspace_methods(&mut self) {
        let _api = self.api.clone();

        // list_workspaces
        let api = self.api.clone();
        self.module
            .register_async_method("list_workspaces", move |_, _, _| {
                let api = api.clone();
                async move {
                    let workspaces = api.list_workspaces().await.map_err(map_error)?;
                    let response: Vec<WorkspaceInfoResponse> = workspaces
                        .into_iter()
                        .map(|w| WorkspaceInfoResponse {
                            workspace_id: w.workspace_id.to_string(),
                            working_dir: w.working_dir,
                            node_count: w.node_count,
                            relation_count: w.relation_count,
                            last_updated: w.last_updated.map(|t| t.to_rfc3339()),
                            created_at: w.created_at.to_rfc3339(),
                        })
                        .collect();
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register list_workspaces");

        // get_workspace_info
        let api = self.api.clone();
        self.module
            .register_async_method("get_workspace_info", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: WorkspacePathParams = params.parse()?;
                    let path = PathBuf::from(&params.path);

                    let info = api.get_workspace_info(path).await.map_err(map_error)?;

                    let response = info.map(|w| WorkspaceInfoResponse {
                        workspace_id: w.workspace_id.to_string(),
                        working_dir: w.working_dir,
                        node_count: w.node_count,
                        relation_count: w.relation_count,
                        last_updated: w.last_updated.map(|t| t.to_rfc3339()),
                        created_at: w.created_at.to_rfc3339(),
                    });

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_workspace_info");

        // delete_workspaces
        let api = self.api.clone();
        self.module
            .register_async_method("delete_workspaces", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: DeleteWorkspacesParams = params.parse()?;

                    let ids: Vec<forge_domain::WorkspaceId> = params
                        .workspace_ids
                        .into_iter()
                        .map(|id| {
                            forge_domain::WorkspaceId::from_string(&id).map_err(|e| {
                                ErrorObjectOwned::owned(
                                    -32602,
                                    format!("Invalid workspace ID: {}", e),
                                    None::<()>,
                                )
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;

                    api.delete_workspaces(ids).await.map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register delete_workspaces");

        // get_workspace_status
        let api = self.api.clone();
        self.module
            .register_async_method("get_workspace_status", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: WorkspacePathParams = params.parse()?;
                    let path = PathBuf::from(&params.path);

                    let statuses = api.get_workspace_status(path).await.map_err(map_error)?;
                    let response: Vec<FileStatusResponse> = statuses
                        .into_iter()
                        .map(|s| FileStatusResponse {
                            path: s.path,
                            status: format!("{:?}", s.status),
                        })
                        .collect();
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_workspace_status");

        // sync_workspace (streaming via subscription)
        let api = self.api.clone();
        self.module
            .register_subscription(
                "sync_workspace.subscribe",
                "sync_workspace.notification",
                "sync_workspace.unsubscribe",
                move |params, pending, _, _| {
                    let api = api.clone();
                    async move {
                        let params: SyncWorkspaceParams = params.parse()?;
                        let path = PathBuf::from(&params.path);

                        let stream = api.sync_workspace(path).await.map_err(map_error)?;
                        let sink = pending.accept().await?;

                        tokio::spawn(async move {
                            let mut stream = stream;
                            while let Some(result) = stream.next().await {
                                let msg = match result {
                                    Ok(progress) => {
                                        let data = match &progress {
                                            forge_domain::SyncProgress::Syncing {
                                                current,
                                                total,
                                            } => {
                                                json!({
                                                    "type": "Syncing",
                                                    "current": current,
                                                    "total": total,
                                                })
                                            }
                                            forge_domain::SyncProgress::Completed {
                                                uploaded_files,
                                                total_files,
                                                failed_files,
                                            } => {
                                                json!({
                                                    "type": "Completed",
                                                    "uploaded_files": uploaded_files,
                                                    "total_files": total_files,
                                                    "failed_files": failed_files,
                                                })
                                            }
                                            forge_domain::SyncProgress::Starting => {
                                                json!({"type": "Starting"})
                                            }
                                            forge_domain::SyncProgress::WorkspaceCreated {
                                                workspace_id,
                                            } => {
                                                json!({
                                                    "type": "WorkspaceCreated",
                                                    "workspace_id": workspace_id.to_string(),
                                                })
                                            }
                                            forge_domain::SyncProgress::DiscoveringFiles {
                                                workspace_id,
                                                path,
                                            } => {
                                                json!({
                                                    "type": "DiscoveringFiles",
                                                    "workspace_id": workspace_id.to_string(),
                                                    "path": path.to_string_lossy().to_string(),
                                                })
                                            }
                                            forge_domain::SyncProgress::FilesDiscovered {
                                                count,
                                            } => {
                                                json!({
                                                    "type": "FilesDiscovered",
                                                    "count": count,
                                                })
                                            }
                                            forge_domain::SyncProgress::ComparingFiles {
                                                remote_files,
                                                local_files,
                                            } => {
                                                json!({
                                                    "type": "ComparingFiles",
                                                    "remote_files": remote_files,
                                                    "local_files": local_files,
                                                })
                                            }
                                            forge_domain::SyncProgress::DiffComputed {
                                                added,
                                                deleted,
                                                modified,
                                            } => {
                                                json!({
                                                    "type": "DiffComputed",
                                                    "added": added,
                                                    "deleted": deleted,
                                                    "modified": modified,
                                                })
                                            }
                                        };
                                        StreamMessage::Chunk { data }
                                    }
                                    Err(e) => StreamMessage::Error { message: format!("{:#}", e) },
                                };

                                let sub_msg =
                                    SubscriptionMessage::from_json(&msg).unwrap_or_else(|_| {
                                        SubscriptionMessage::from_json(&json!({"status": "error"}))
                                            .expect("fallback message should never fail")
                                    });
                                if sink.send(sub_msg).await.is_err() {
                                    debug!("Client disconnected from sync_workspace stream");
                                    break;
                                }
                            }

                            let complete_msg =
                                SubscriptionMessage::from_json(&StreamMessage::Complete)
                                    .unwrap_or_else(|_| {
                                        SubscriptionMessage::from_json(
                                            &json!({"status": "complete"}),
                                        )
                                        .unwrap_or_else(
                                            |_| {
                                                SubscriptionMessage::from_json(
                                                    &json!({"done": true}),
                                                )
                                                .expect("fallback message should never fail")
                                            },
                                        )
                                    });
                            let _ = sink.send(complete_msg).await;
                        });

                        Ok(())
                    }
                },
            )
            .expect("Failed to register sync_workspace subscription");

        // query_workspace
        let api = self.api.clone();
        self.module
            .register_async_method("query_workspace", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: QueryWorkspaceParams = params.parse()?;
                    let path = PathBuf::from(&params.path);

                    let search_params =
                        forge_domain::SearchParams::new(&params.query, "semantic search")
                            .limit(params.limit.unwrap_or(10));

                    let nodes = api
                        .query_workspace(path, search_params)
                        .await
                        .map_err(map_error)?;

                    let response: Vec<NodeResponse> = nodes
                        .into_iter()
                        .map(|n| {
                            let (path, content) = match &n.node {
                                forge_domain::NodeData::FileChunk(chunk) => {
                                    (Some(chunk.file_path.clone()), Some(chunk.content.clone()))
                                }
                                forge_domain::NodeData::File(file) => {
                                    (Some(file.file_path.clone()), Some(file.content.clone()))
                                }
                                forge_domain::NodeData::FileRef(file_ref) => {
                                    (Some(file_ref.file_path.clone()), None)
                                }
                                forge_domain::NodeData::Note(note) => {
                                    (None, Some(note.content.clone()))
                                }
                                forge_domain::NodeData::Task(task) => {
                                    (None, Some(task.task.clone()))
                                }
                            };
                            NodeResponse {
                                node_id: n.node_id.to_string(),
                                path,
                                content,
                                relevance: n.relevance,
                                distance: n.distance,
                            }
                        })
                        .collect();

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register query_workspace");
    }

    /// Register configuration methods
    fn register_config_methods(&mut self) {
        let _api = self.api.clone();

        // read_mcp_config
        let api = self.api.clone();
        self.module
            .register_async_method("read_mcp_config", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: McpConfigParams = params.parse()?;
                    let scope = params.scope.map(|s| match s.as_str() {
                        "user" => Scope::User,
                        "project" | "local" => Scope::Local,
                        _ => Scope::Local,
                    });

                    let config = api
                        .read_mcp_config(scope.as_ref())
                        .await
                        .map_err(map_error)?;
                    let response = config;
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register read_mcp_config");

        // write_mcp_config
        let api = self.api.clone();
        self.module
            .register_async_method("write_mcp_config", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: WriteMcpConfigParams = params.parse()?;
                    let scope = match params.scope.as_str() {
                        "user" => Scope::User,
                        "project" | "local" => Scope::Local,
                        _ => Scope::Local,
                    };

                    let config: McpConfig = serde_json::from_value(params.config).map_err(|e| {
                        ErrorObjectOwned::owned(
                            ErrorCode::INVALID_PARAMS,
                            format!("Invalid config: {}", e),
                            None::<()>,
                        )
                    })?;

                    api.write_mcp_config(&scope, &config)
                        .await
                        .map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register write_mcp_config");

        // update_config
        let api = self.api.clone();
        self.module
            .register_async_method("update_config", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: ConfigParams = params.parse()?;

                    // Convert typed DTOs to domain ConfigOperations
                    let ops: Vec<forge_domain::ConfigOperation> = params
                        .ops
                        .into_iter()
                        .map(|op| {
                            op.into_domain().map_err(|e| {
                                ErrorObjectOwned::owned(
                                    ErrorCode::INVALID_PARAMS,
                                    format!("Invalid config operation: {}", e),
                                    None::<()>,
                                )
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;

                    api.update_config(ops).await.map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register update_config");

        // get_commit_config
        let api = self.api.clone();
        self.module
            .register_async_method("get_commit_config", move |_, _, _| {
                let api = api.clone();
                async move {
                    let config = api.get_commit_config().await.map_err(map_error)?;
                    let json = serde_json::to_value(&config).unwrap_or(json!(null));
                    Ok::<_, ErrorObjectOwned>(json)
                }
            })
            .expect("Failed to register get_commit_config");

        // get_suggest_config
        let api = self.api.clone();
        self.module
            .register_async_method("get_suggest_config", move |_, _, _| {
                let api = api.clone();
                async move {
                    let config = api.get_suggest_config().await.map_err(map_error)?;
                    let json = serde_json::to_value(&config).unwrap_or(json!(null));
                    Ok::<_, ErrorObjectOwned>(json)
                }
            })
            .expect("Failed to register get_suggest_config");

        // get_reasoning_effort
        let api = self.api.clone();
        self.module
            .register_async_method("get_reasoning_effort", move |_, _, _| {
                let api = api.clone();
                async move {
                    let effort = api.get_reasoning_effort().await.map_err(map_error)?;
                    let json = serde_json::to_value(&effort).unwrap_or(json!(null));
                    Ok::<_, ErrorObjectOwned>(json)
                }
            })
            .expect("Failed to register get_reasoning_effort");

        // reload_mcp
        let api = self.api.clone();
        self.module
            .register_async_method("reload_mcp", move |_, _, _| {
                let api = api.clone();
                async move {
                    api.reload_mcp().await.map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register reload_mcp");

        // get_active_agent
        let api = self.api.clone();
        self.module
            .register_async_method("get_active_agent", move |_, _, _| {
                let api = api.clone();
                async move {
                    let agent_id = api.get_active_agent().await;
                    let json = serde_json::to_value(agent_id.map(|id| id.to_string()))
                        .unwrap_or(json!(null));
                    Ok::<_, ErrorObjectOwned>(json)
                }
            })
            .expect("Failed to register get_active_agent");

        // set_active_agent
        let api = self.api.clone();
        self.module
            .register_async_method("set_active_agent", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: SetActiveAgentParams = params.parse()?;
                    let agent_id = AgentId::new(&params.agent_id);

                    api.set_active_agent(agent_id).await.map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register set_active_agent");

        // get_agent_model
        let api = self.api.clone();
        self.module
            .register_async_method("get_agent_model", move |params, _, _| {
                let api = api.clone();
                async move {
                    let agent_id_str: String = params.parse()?;
                    let agent_id = AgentId::new(&agent_id_str);

                    let model_id = api.get_agent_model(agent_id).await;
                    let json = serde_json::to_value(model_id.map(|id| id.to_string()))
                        .unwrap_or(json!(null));
                    Ok::<_, ErrorObjectOwned>(json)
                }
            })
            .expect("Failed to register get_agent_model");

        // get_default_model
        let api = self.api.clone();
        self.module
            .register_async_method("get_default_model", move |_, _, _| {
                let api = api.clone();
                async move {
                    let model_id = api.get_default_model().await;
                    let json = serde_json::to_value(model_id.map(|id| id.to_string()))
                        .unwrap_or(json!(null));
                    Ok::<_, ErrorObjectOwned>(json)
                }
            })
            .expect("Failed to register get_default_model");
    }

    /// Register authentication and user methods
    fn register_auth_methods(&mut self) {
        let _api = self.api.clone();

        // user_info
        let api = self.api.clone();
        self.module
            .register_async_method("user_info", move |_, _, _| {
                let api = api.clone();
                async move {
                    let user_info = api.user_info().await.map_err(map_error)?;

                    let response = user_info.map(|u| UserInfoResponse {
                        auth_provider_id: u.auth_provider_id.into_string(),
                    });

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register user_info");

        // user_usage
        let api = self.api.clone();
        self.module
            .register_async_method("user_usage", move |_, _, _| {
                let api = api.clone();
                async move {
                    let usage = api.user_usage().await.map_err(map_error)?;

                    let response = usage.map(|u| UserUsageResponse {
                        plan_type: u.plan.r#type,
                        current: u.usage.current,
                        limit: u.usage.limit,
                        remaining: u.usage.remaining,
                        reset_in: u.usage.reset_in,
                    });

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register user_usage");

        // is_authenticated
        let api = self.api.clone();
        self.module
            .register_async_method("is_authenticated", move |_, _, _| {
                let api = api.clone();
                async move {
                    let authenticated = api.is_authenticated().await.map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!(authenticated))
                }
            })
            .expect("Failed to register is_authenticated");

        // init_provider_auth
        let api = self.api.clone();
        self.module
            .register_async_method("init_provider_auth", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: ProviderAuthParams = params.parse()?;
                    let provider_id = ProviderId::from_str(&params.provider_id).map_err(|_| {
                        ErrorObjectOwned::owned(-32602, "Invalid provider ID", None::<()>,)
                    })?;
                    // Parse the auth method - for OAuth variants we need a config which isn't provided here
                    // So we'll return an error for OAuth or default to ApiKey
                    let method = match params.method.as_str() {
                        "api_key" => forge_domain::AuthMethod::ApiKey,
                        "google_adc" => forge_domain::AuthMethod::GoogleAdc,
                        _ => return Err(ErrorObjectOwned::owned(
                            -32602,
                            format!("Auth method '{}' requires OAuth config which is not supported via JSON-RPC. Use 'api_key' or 'google_adc'.", params.method),
                            None::<()>,
                        )),
                    };

                    let context = api.init_provider_auth(provider_id, method).await.map_err(map_error)?;

                    let response = match context {
                        forge_domain::AuthContextRequest::ApiKey(req) => AuthContextRequestResponse {
                            url: None,
                            message: Some(format!("API Key required. Required params: {:?}", req.required_params)),
                        },
                        forge_domain::AuthContextRequest::DeviceCode(req) => AuthContextRequestResponse {
                            url: Some(req.verification_uri.to_string()),
                            message: Some(format!("User code: {}. Please visit the URL to authenticate.", req.user_code)),
                        },
                        forge_domain::AuthContextRequest::Code(req) => AuthContextRequestResponse {
                            url: Some(req.authorization_url.to_string()),
                            message: Some(format!("Please visit the URL to authenticate. State: {:?}", req.state)),
                        },
                    };

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register init_provider_auth");

        // complete_provider_auth - Complete provider authentication and save
        // credentials
        let api = self.api.clone();
        self.module
            .register_async_method("complete_provider_auth", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: CompleteProviderAuthParams = params.parse()?;
                    let provider_id = ProviderId::from_str(&params.provider_id).map_err(|_| {
                        ErrorObjectOwned::owned(-32602, "Invalid provider ID", None::<()>)
                    })?;

                    // Build the appropriate AuthContextResponse based on flow type
                    let context_response = match params.flow_type.as_str() {
                        "api_key" => {
                            let api_key = params.api_key.ok_or_else(|| {
                                ErrorObjectOwned::owned(
                                    -32602,
                                    "api_key field is required for api_key flow",
                                    None::<()>,
                                )
                            })?;
                            // Parse URL params if provided
                            let url_params = params.url_params.unwrap_or_default();
                            let url_params_map: std::collections::HashMap<_, _> = url_params
                                .into_iter()
                                .map(|(k, v)| {
                                    (forge_domain::URLParam::from(k), forge_domain::URLParamValue::from(v))
                                })
                                .collect();
                            // Build the API key request
                            let api_key_request = forge_domain::ApiKeyRequest {
                                required_params: vec![],
                                existing_params: None,
                                api_key: None,
                            };
                            forge_domain::AuthContextResponse::api_key(
                                api_key_request,
                                api_key,
                                url_params_map.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                            )
                        }
                        "device_code" => {
                            // Device code requires the original request which we don't have
                            // For JSON-RPC, this is a limitation - the client would need to 
                            // complete device code flow out-of-band
                            return Err(ErrorObjectOwned::owned(
                                -32603,
                                "Device code flow completion requires the original request context which is not available via JSON-RPC. Please use the web UI or CLI for device code authentication.",
                                None::<()>,
                            ));
                        }
                        "code" => {
                            let _code = params.code.ok_or_else(|| {
                                ErrorObjectOwned::owned(
                                    -32602,
                                    "code field is required for authorization_code flow",
                                    None::<()>,
                                )
                            })?;
                            // Authorization code flow also requires the original request
                            return Err(ErrorObjectOwned::owned(
                                -32603,
                                "Authorization code flow completion requires the original request context (PKCE verifier, state) which is not available via JSON-RPC. Please use the web UI or CLI for OAuth authentication.",
                                None::<()>,
                            ));
                        }
                        _ => {
                            return Err(ErrorObjectOwned::owned(
                                -32602,
                                format!("Unknown flow_type: {}. Supported: api_key", params.flow_type),
                                None::<()>,
                            ));
                        }
                    };

                    let timeout = std::time::Duration::from_secs(params.timeout_seconds.unwrap_or(60));

                    api.complete_provider_auth(provider_id, context_response, timeout)
                        .await
                        .map_err(map_error)?;

                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register complete_provider_auth");

        // remove_provider
        let api = self.api.clone();
        self.module
            .register_async_method("remove_provider", move |params, _, _| {
                let api = api.clone();
                async move {
                    let provider_id_str: String = params.parse()?;
                    let provider_id = ProviderId::from_str(&provider_id_str).map_err(|_| {
                        ErrorObjectOwned::owned(-32602, "Invalid provider ID", None::<()>)
                    })?;

                    api.remove_provider(&provider_id).await.map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register remove_provider");

        // create_auth_credentials
        let api = self.api.clone();
        self.module
            .register_async_method("create_auth_credentials", move |_, _, _| {
                let api = api.clone();
                async move {
                    let auth = api.create_auth_credentials().await.map_err(map_error)?;
                    let response = auth;
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register create_auth_credentials");

        // migrate_env_credentials
        let api = self.api.clone();
        self.module
            .register_async_method("migrate_env_credentials", move |_, _, _| {
                let api = api.clone();
                async move {
                    let result = api.migrate_env_credentials().await.map_err(map_error)?;
                    let json_value = result.map(|r| json!({
                        "credentials_path": r.credentials_path.to_string_lossy().to_string(),
                        "migrated_providers": r.migrated_providers.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
                    })).unwrap_or(json!(null));
                    Ok::<_, ErrorObjectOwned>(json_value)
                }
            })
            .expect("Failed to register migrate_env_credentials");

        // mcp_auth
        let api = self.api.clone();
        self.module
            .register_async_method("mcp_auth", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: McpAuthParams = params.parse()?;

                    api.mcp_auth(&params.server_url).await.map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register mcp_auth");

        // mcp_logout
        let api = self.api.clone();
        self.module
            .register_async_method("mcp_logout", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: McpLogoutParams = params.parse()?;

                    api.mcp_logout(params.server_url.as_deref())
                        .await
                        .map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register mcp_logout");

        // mcp_auth_status
        let api = self.api.clone();
        self.module
            .register_async_method("mcp_auth_status", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: McpAuthStatusParams = params.parse()?;

                    let status = api
                        .mcp_auth_status(&params.server_url)
                        .await
                        .map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!(status))
                }
            })
            .expect("Failed to register mcp_auth_status");
    }

    /// Register system methods (execute_shell_command, get_commands,
    /// get_skills, etc.)
    fn register_system_methods(&mut self) {
        // execute_shell_command
        let api = self.api.clone();
        self.module
            .register_async_method("execute_shell_command", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: ShellCommandParams = params.parse()?;
                    let working_dir = params
                        .working_dir
                        .map(PathBuf::from)
                        .unwrap_or_else(|| api.environment().cwd.clone());

                    let output = api
                        .execute_shell_command(&params.command, working_dir)
                        .await
                        .map_err(map_error)?;

                    let response = CommandOutputResponse {
                        stdout: output.stdout,
                        stderr: output.stderr,
                        exit_code: output.exit_code,
                    };

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register execute_shell_command");

        // execute_shell_command_raw - executes shell command on present stdio
        let api = self.api.clone();
        self.module
            .register_async_method("execute_shell_command_raw", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: ShellCommandParams = params.parse()?;

                    let exit_status = api
                        .execute_shell_command_raw(&params.command)
                        .await
                        .map_err(map_error)?;

                    // Convert ExitStatus to a simple integer response
                    let exit_code = exit_status.code();
                    #[cfg(unix)]
                    let signal = exit_status.signal();
                    #[cfg(not(unix))]
                    let signal: Option<i32> = None;
                    let response = json!({
                        "success": exit_status.success(),
                        "exit_code": exit_code,
                        "signal": signal,
                    });

                    Ok::<_, ErrorObjectOwned>(response)
                }
            })
            .expect("Failed to register execute_shell_command_raw");

        // get_commands
        let api = self.api.clone();
        self.module
            .register_async_method("get_commands", move |_, _, _| {
                let api = api.clone();
                async move {
                    let commands = api.get_commands().await.map_err(map_error)?;
                    let response: Vec<CommandResponse> = commands
                        .into_iter()
                        .map(|c| CommandResponse { name: c.name, description: c.description })
                        .collect();
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_commands");

        // get_skills
        let api = self.api.clone();
        self.module
            .register_async_method("get_skills", move |_, _, _| {
                let api = api.clone();
                async move {
                    let skills = api.get_skills().await.map_err(map_error)?;
                    let response: Vec<SkillResponse> = skills
                        .into_iter()
                        .map(|s| SkillResponse {
                            name: s.name,
                            path: s.path.map(|p| p.to_string_lossy().to_string()),
                            command: s.command,
                            description: s.description,
                            resources: Some(
                                s.resources
                                    .iter()
                                    .map(|r| r.to_string_lossy().to_string())
                                    .collect(),
                            )
                            .filter(|r: &Vec<String>| !r.is_empty()),
                        })
                        .collect();
                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register get_skills");

        // generate_command
        let api = self.api.clone();
        self.module
            .register_async_method("generate_command", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: GenerateCommandParams = params.parse()?;
                    let prompt: UserPrompt = params.prompt.into();

                    let command = api.generate_command(prompt).await.map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!(command))
                }
            })
            .expect("Failed to register generate_command");

        // commit
        let api = self.api.clone();
        self.module
            .register_async_method("commit", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: CommitParams = params.parse()?;

                    let result = api
                        .commit(
                            params.preview,
                            params.max_diff_size,
                            params.diff,
                            params.additional_context,
                        )
                        .await
                        .map_err(map_error)?;

                    let response = CommitResultResponse {
                        message: result.message,
                        has_staged_files: result.has_staged_files,
                    };

                    Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
                }
            })
            .expect("Failed to register commit");

        // init_workspace
        let api = self.api.clone();
        self.module
            .register_async_method("init_workspace", move |params, _, _| {
                let api = api.clone();
                async move {
                    let params: InitWorkspaceParams = params.parse()?;
                    let path = PathBuf::from(&params.path);

                    let workspace_id = api.init_workspace(path).await.map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!(workspace_id.to_string()))
                }
            })
            .expect("Failed to register init_workspace");

        // environment
        let api = self.api.clone();
        self.module
            .register_method("environment", move |_, _, _| {
                let env = api.environment();
                let response = env;
                Ok::<_, ErrorObjectOwned>(to_json_response(response)?)
            })
            .expect("Failed to register environment");

        // hydrate_channel
        let api = self.api.clone();
        self.module
            .register_async_method("hydrate_channel", move |_, _, _| {
                let api = api.clone();
                async move {
                    api.hydrate_channel().map_err(map_error)?;
                    Ok::<_, ErrorObjectOwned>(json!({ "success": true }))
                }
            })
            .expect("Failed to register hydrate_channel");

        // generate_data (streaming via subscription)
        let api = self.api.clone();
        self.module
            .register_subscription(
                "generate_data.subscribe",
                "generate_data.notification",
                "generate_data.unsubscribe",
                move |params, pending, _, _| {
                    let api = api.clone();
                    async move {
                        let params: GenerateDataParams = params.parse()?;

                        let data_params = DataGenerationParameters {
                            input: PathBuf::from(params.input),
                            schema: PathBuf::from(params.schema),
                            system_prompt: params.system_prompt.map(PathBuf::from),
                            user_prompt: params.user_prompt.map(PathBuf::from),
                            concurrency: params.concurrency.unwrap_or(1),
                        };

                        let stream = api.generate_data(data_params).await.map_err(map_error)?;
                        let sink = pending.accept().await?;

                        tokio::spawn(async move {
                            let mut stream = stream;
                            while let Some(result) = stream.next().await {
                                let msg = match result {
                                    Ok(data) => json!({"type": "chunk", "data": data}),
                                    Err(e) => {
                                        json!({"type": "error", "message": format!("{:#}", e)})
                                    }
                                };

                                let sub_msg = match SubscriptionMessage::from_json(&msg) {
                                    Ok(m) => m,
                                    Err(_) => continue,
                                };

                                if sink.send(sub_msg).await.is_err() {
                                    debug!("Client disconnected from generate_data stream");
                                    break;
                                }
                            }

                            if let Ok(msg) =
                                SubscriptionMessage::from_json(&json!({"type": "complete"}))
                            {
                                let _ = sink.send(msg).await;
                            }
                        });

                        Ok(())
                    }
                },
            )
            .expect("Failed to register generate_data subscription");
    }

    /// Run the JSON-RPC server over STDIO (stdin/stdout)
    ///
    /// This is the pure STDIO transport with zero TCP overhead.
    /// Directly reads JSON-RPC requests from stdin and writes responses to
    /// stdout using the RpcModule without any intermediate TCP server.
    pub async fn run_stdio(self) -> anyhow::Result<()> {
        let transport = StdioTransport::new(self.module);
        transport.run().await
    }

    #[doc(hidden)]
    /// Get the RPC module for testing purposes
    pub fn into_module(self) -> RpcModule<()> {
        self.module
    }
}
/// Helper function to convert nested method categories to a flat array
fn get_all_jsonrpc_methods() -> Vec<Value> {
    let method_categories = get_method_list();
    let mut methods = Vec::new();

    if let Some(obj) = method_categories.as_object() {
        for (category, category_methods) in obj {
            if let Some(cat_obj) = category_methods.as_object() {
                for (method_name, method_def) in cat_obj {
                    let mut method_entry = method_def.clone();
                    if let Some(obj) = method_entry.as_object_mut() {
                        obj.insert("name".to_string(), json!(method_name));
                        obj.insert("category".to_string(), json!(category));
                    }
                    methods.push(method_entry);
                }
            }
        }
    }

    methods
}

/// Helper function to convert type definitions to a flat array
fn get_all_types() -> Vec<Value> {
    let type_defs = get_type_definitions();
    let mut types = Vec::new();

    if let Some(obj) = type_defs.as_object() {
        for (type_name, type_def) in obj {
            let mut type_entry = type_def.clone();
            if let Some(obj) = type_entry.as_object_mut() {
                obj.insert("name".to_string(), json!(type_name));
            }
            types.push(type_entry);
        }
    }

    types
}

/// Returns the complete API schema as a JSON value
fn get_api_schema() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "info": {
            "title": "Forge JSON-RPC API",
            "version": "1.0.0",
            "description": "JSON-RPC API for Forge - AI-powered coding assistant"
        },
        "methods": get_method_list(),
        "types": get_type_definitions()
    })
}

/// Returns a list of all available JSON-RPC methods with their signatures
fn get_method_list() -> Value {
    serde_json::json!({
        "discovery": {
            "get_models": {
                "description": "Get all available models",
                "params": null,
                "returns": { "type": "array", "items": { "$ref": "ModelResponse" } }
            },
            "get_agents": {
                "description": "List all agents",
                "params": null,
                "returns": { "type": "array", "items": { "$ref": "AgentResponse" } }
            },
            "get_tools": {
                "description": "List system/agent/MCP tools",
                "params": null,
                "returns": { "$ref": "ToolsOverviewResponse" }
            },
            "discover": {
                "description": "Discover files in workspace",
                "params": null,
                "returns": { "type": "array", "items": { "$ref": "FileResponse" } }
            },
            "get_providers": {
                "description": "List all configured providers",
                "params": null,
                "returns": { "type": "array", "items": { "$ref": "ProviderResponse" } }
            },
            "get_all_provider_models": {
                "description": "Get models grouped by provider",
                "params": null,
                "returns": { "type": "array", "items": { "$ref": "ProviderModelsResponse" } }
            },
            "get_provider": {
                "description": "Get a single provider by ID",
                "params": { "type": "string" },
                "returns": { "$ref": "ProviderResponse" }
            },
            "get_agent_provider": {
                "description": "Get provider for a specific agent",
                "params": { "type": "string" },
                "returns": { "$ref": "ProviderResponse" }
            },
            "get_default_provider": {
                "description": "Get the default provider",
                "params": null,
                "returns": { "$ref": "ProviderResponse" }
            },
            "get_schema": {
                "description": "Get the complete API schema",
                "params": null,
                "returns": { "type": "object" }
            },
            "get_methods": {
                "description": "Get list of all available methods",
                "params": null,
                "returns": { "type": "object" }
            },
            "get_types": {
                "description": "Get all type definitions",
                "params": null,
                "returns": { "type": "object" }
            }
        },
        "chat": {
            "chat.stream": {
                "description": "Subscribe to real-time chat message stream via notifications",
                "params": { "$ref": "ChatParams" },
                "returns": { "type": "subscription" },
                "notifications": ["chat.notification"]
            }
        },
        "conversations": {
            "get_conversations": {
                "description": "List all conversations",
                "params": null,
                "returns": { "type": "array", "items": { "$ref": "ConversationResponse" } }
            },
            "conversation": {
                "description": "Get a specific conversation",
                "params": { "type": "object", "properties": { "conversation_id": { "type": "string" } } },
                "returns": { "$ref": "ConversationResponse" }
            },
            "upsert_conversation": {
                "description": "Create or update a conversation",
                "params": { "type": "object" },
                "returns": { "$ref": "ConversationResponse" }
            },
            "delete_conversation": {
                "description": "Delete a conversation",
                "params": { "type": "object", "properties": { "conversation_id": { "type": "string" } } },
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "rename_conversation": {
                "description": "Rename a conversation",
                "params": { "$ref": "RenameConversationParams" },
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "last_conversation": {
                "description": "Get the most recent conversation",
                "params": null,
                "returns": { "$ref": "ConversationResponse" }
            },
            "compact_conversation": {
                "description": "Compact a conversation",
                "params": { "type": "object", "properties": { "conversation_id": { "type": "string" } } },
                "returns": { "$ref": "CompactionResultResponse" }
            }
        },
        "workspace": {
            "list_workspaces": {
                "description": "List all workspaces",
                "params": null,
                "returns": { "type": "array", "items": { "$ref": "WorkspaceInfoResponse" } }
            },
            "get_workspace_info": {
                "description": "Get workspace info by path",
                "params": { "type": "object", "properties": { "path": { "type": "string" } } },
                "returns": { "$ref": "WorkspaceInfoResponse" }
            },
            "delete_workspaces": {
                "description": "Delete workspaces",
                "params": { "type": "object", "properties": { "workspace_ids": { "type": "array", "items": { "type": "string" } } } },
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "get_workspace_status": {
                "description": "Get workspace status",
                "params": { "type": "object", "properties": { "path": { "type": "string" } } },
                "returns": { "type": "array", "items": { "$ref": "FileStatusResponse" } }
            },
            "sync_workspace.subscribe": {
                "description": "Subscribe to workspace sync progress",
                "params": { "$ref": "SyncWorkspaceParams" },
                "returns": { "type": "subscription" },
                "notifications": ["sync_workspace.notification"]
            },
            "query_workspace": {
                "description": "Query workspace files",
                "params": { "$ref": "QueryWorkspaceParams" },
                "returns": { "type": "array", "items": { "$ref": "NodeResponse" } }
            }
        },
        "config": {
            "read_mcp_config": {
                "description": "Read MCP configuration",
                "params": { "type": "object", "properties": { "scope": { "type": "string" } } },
                "returns": { "type": "object" }
            },
            "write_mcp_config": {
                "description": "Write MCP configuration",
                "params": { "type": "object", "properties": { "scope": { "type": "string" }, "config": { "type": "object" } } },
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "update_config": {
                "description": "Update configuration",
                "params": { "type": "object", "properties": { "ops": { "type": "array" } } },
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "get_commit_config": {
                "description": "Get commit configuration",
                "params": null,
                "returns": { "type": "object" }
            },
            "get_suggest_config": {
                "description": "Get suggest configuration",
                "params": null,
                "returns": { "type": "object" }
            },
            "get_reasoning_effort": {
                "description": "Get reasoning effort level",
                "params": null,
                "returns": { "type": "string" }
            },
            "reload_mcp": {
                "description": "Reload MCP configuration",
                "params": null,
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "get_active_agent": {
                "description": "Get the currently active agent",
                "params": null,
                "returns": { "type": "string" }
            },
            "set_active_agent": {
                "description": "Set the active agent",
                "params": { "type": "object", "properties": { "agent_id": { "type": "string" } } },
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "get_agent_model": {
                "description": "Get the model for an agent",
                "params": { "type": "string" },
                "returns": { "type": "string" }
            },
            "get_default_model": {
                "description": "Get the default model",
                "params": null,
                "returns": { "type": "string" }
            }
        },
        "auth": {
            "user_info": {
                "description": "Get authenticated user info",
                "params": null,
                "returns": { "$ref": "UserInfoResponse" }
            },
            "user_usage": {
                "description": "Get user usage information",
                "params": null,
                "returns": { "$ref": "UserUsageResponse" }
            },
            "is_authenticated": {
                "description": "Check if user is authenticated",
                "params": null,
                "returns": { "type": "boolean" }
            },
            "init_provider_auth": {
                "description": "Initialize provider authentication",
                "params": { "type": "object", "properties": { "provider_id": { "type": "string" }, "method": { "type": "string" } } },
                "returns": { "$ref": "AuthContextRequestResponse" }
            },
            "remove_provider": {
                "description": "Remove a provider",
                "params": { "type": "string" },
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "create_auth_credentials": {
                "description": "Create authentication credentials",
                "params": null,
                "returns": { "type": "object" }
            },
            "migrate_env_credentials": {
                "description": "Migrate environment credentials",
                "params": null,
                "returns": { "type": "object" }
            },
            "mcp_auth": {
                "description": "Authenticate with MCP server",
                "params": { "type": "object", "properties": { "server_url": { "type": "string" } } },
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "mcp_logout": {
                "description": "Logout from MCP server",
                "params": { "type": "object", "properties": { "server_url": { "type": "string" } } },
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "mcp_auth_status": {
                "description": "Get MCP authentication status",
                "params": { "type": "object", "properties": { "server_url": { "type": "string" } } },
                "returns": { "type": "boolean" }
            }
        },
        "system": {
            "execute_shell_command": {
                "description": "Execute a shell command",
                "params": { "$ref": "ShellCommandParams" },
                "returns": { "$ref": "CommandOutputResponse" }
            },
            "get_commands": {
                "description": "List available commands",
                "params": null,
                "returns": { "type": "array", "items": { "$ref": "CommandResponse" } }
            },
            "get_skills": {
                "description": "List available skills",
                "params": null,
                "returns": { "type": "array", "items": { "$ref": "SkillResponse" } }
            },
            "generate_command": {
                "description": "Generate a command from a prompt",
                "params": { "type": "object", "properties": { "prompt": { "type": "string" } } },
                "returns": { "type": "string" }
            },
            "commit": {
                "description": "Generate a commit message",
                "params": { "$ref": "CommitParams" },
                "returns": { "$ref": "CommitResultResponse" }
            },
            "init_workspace": {
                "description": "Initialize a workspace",
                "params": { "type": "object", "properties": { "path": { "type": "string" } } },
                "returns": { "type": "string" }
            },
            "environment": {
                "description": "Get environment information",
                "params": null,
                "returns": { "type": "object" }
            },
            "hydrate_channel": {
                "description": "Hydrate the command channel",
                "params": null,
                "returns": { "type": "object", "properties": { "success": { "type": "boolean" } } }
            },
            "generate_data.subscribe": {
                "description": "Subscribe to data generation stream",
                "params": { "$ref": "GenerateDataParams" },
                "returns": { "type": "subscription" },
                "notifications": ["generate_data.notification"]
            }
        }
    })
}

/// Returns all type definitions for the API
fn get_type_definitions() -> Value {
    serde_json::json!({
        "ModelResponse": {
            "description": "Model information",
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Model ID" },
                "name": { "type": "string", "description": "Model name" },
                "provider": { "type": "string", "description": "Provider ID" }
            }
        },
        "AgentResponse": {
            "description": "Agent information",
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Agent ID" },
                "name": { "type": "string", "description": "Agent name" },
                "description": { "type": "string", "nullable": true, "description": "Agent description" }
            }
        },
        "ToolsOverviewResponse": {
            "description": "Tools overview",
            "type": "object",
            "properties": {
                "enabled": { "type": "array", "items": { "type": "string" }, "description": "Enabled tool names" },
                "disabled": { "type": "array", "items": { "type": "string" }, "description": "Disabled tool names" }
            }
        },
        "FileResponse": {
            "description": "File information",
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path" },
                "is_dir": { "type": "boolean", "description": "Whether this is a directory" }
            }
        },
        "ProviderResponse": {
            "description": "Provider information",
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Provider ID" },
                "name": { "type": "string", "description": "Provider name" },
                "api_key": { "type": "string", "nullable": true, "description": "API key (if available)" }
            }
        },
        "ProviderModelsResponse": {
            "description": "Provider with its models",
            "type": "object",
            "properties": {
                "provider_id": { "type": "string", "description": "Provider ID" },
                "provider_name": { "type": "string", "description": "Provider name" },
                "models": { "type": "array", "items": { "$ref": "ModelResponse" }, "description": "Available models" },
                "error": { "type": "string", "nullable": true, "description": "Error message if failed to fetch models" }
            }
        },
        "ConversationResponse": {
            "description": "Conversation information",
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Conversation ID" },
                "title": { "type": "string", "nullable": true, "description": "Conversation title" },
                "created_at": { "type": "string", "format": "date-time", "description": "Creation timestamp" },
                "updated_at": { "type": "string", "format": "date-time", "nullable": true, "description": "Last update timestamp" },
                "message_count": { "type": "integer", "nullable": true, "description": "Number of messages" }
            }
        },
        "CompactionResultResponse": {
            "description": "Conversation compaction result",
            "type": "object",
            "properties": {
                "original_tokens": { "type": "integer", "description": "Original token count" },
                "compacted_tokens": { "type": "integer", "description": "Compacted token count" },
                "original_messages": { "type": "integer", "description": "Original message count" },
                "compacted_messages": { "type": "integer", "description": "Compacted message count" }
            }
        },
        "WorkspaceInfoResponse": {
            "description": "Workspace information",
            "type": "object",
            "properties": {
                "workspace_id": { "type": "string", "description": "Workspace ID" },
                "working_dir": { "type": "string", "description": "Working directory" },
                "node_count": { "type": "integer", "nullable": true, "description": "Number of nodes" },
                "relation_count": { "type": "integer", "nullable": true, "description": "Number of relations" },
                "last_updated": { "type": "string", "format": "date-time", "nullable": true, "description": "Last update timestamp" },
                "created_at": { "type": "string", "format": "date-time", "description": "Creation timestamp" }
            }
        },
        "FileStatusResponse": {
            "description": "File status in workspace",
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path" },
                "status": { "type": "string", "description": "File status" }
            }
        },
        "NodeResponse": {
            "description": "Search result node",
            "type": "object",
            "properties": {
                "node_id": { "type": "string", "description": "Node ID" },
                "path": { "type": "string", "nullable": true, "description": "File path" },
                "content": { "type": "string", "nullable": true, "description": "Node content" },
                "relevance": { "type": "number", "nullable": true, "description": "Relevance score" },
                "distance": { "type": "number", "nullable": true, "description": "Distance score" }
            }
        },
        "UserInfoResponse": {
            "description": "User information",
            "type": "object",
            "properties": {
                "auth_provider_id": { "type": "string", "description": "Authentication provider ID" }
            }
        },
        "UserUsageResponse": {
            "description": "User usage information",
            "type": "object",
            "properties": {
                "plan_type": { "type": "string", "description": "Plan type" },
                "current": { "type": "integer", "description": "Current usage" },
                "limit": { "type": "integer", "description": "Usage limit" },
                "remaining": { "type": "integer", "description": "Remaining quota" },
                "reset_in": { "type": "integer", "nullable": true, "description": "Seconds until reset" }
            }
        },
        "CommandResponse": {
            "description": "Available command",
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Command name" },
                "description": { "type": "string", "description": "Command description" }
            }
        },
        "SkillResponse": {
            "description": "Available skill",
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Skill name" },
                "path": { "type": "string", "description": "Skill path" },
                "command": { "type": "string", "description": "Command to invoke skill" },
                "description": { "type": "string", "description": "Skill description" },
                "resources": { "type": "array", "items": { "type": "string" }, "nullable": true, "description": "Skill resources" }
            }
        },
        "CommandOutputResponse": {
            "description": "Shell command output",
            "type": "object",
            "properties": {
                "stdout": { "type": "string", "description": "Standard output" },
                "stderr": { "type": "string", "description": "Standard error" },
                "exit_code": { "type": "integer", "description": "Exit code" }
            }
        },
        "CommitResultResponse": {
            "description": "Commit generation result",
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Generated commit message" },
                "has_staged_files": { "type": "boolean", "description": "Whether there are staged files" }
            }
        },
        "AuthContextRequestResponse": {
            "description": "Authentication context request",
            "type": "object",
            "properties": {
                "url": { "type": "string", "nullable": true, "description": "Authentication URL" },
                "message": { "type": "string", "nullable": true, "description": "Authentication message" }
            }
        },
        "ChatParams": {
            "description": "Chat subscription parameters",
            "type": "object",
            "properties": {
                "event": { "type": "object", "description": "Chat event" },
                "conversation_id": { "type": "string", "description": "Conversation ID" }
            },
            "required": ["event", "conversation_id"]
        },
        "RenameConversationParams": {
            "description": "Rename conversation parameters",
            "type": "object",
            "properties": {
                "conversation_id": { "type": "string", "description": "Conversation ID" },
                "title": { "type": "string", "description": "New title" }
            },
            "required": ["conversation_id", "title"]
        },
        "SyncWorkspaceParams": {
            "description": "Sync workspace parameters",
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Workspace path" }
            }
        },
        "QueryWorkspaceParams": {
            "description": "Query workspace parameters",
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "path": { "type": "string", "description": "Workspace path" },
                "limit": { "type": "integer", "nullable": true, "description": "Result limit" }
            },
            "required": ["query"]
        },
        "ShellCommandParams": {
            "description": "Shell command parameters",
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Command to execute" },
                "working_dir": { "type": "string", "nullable": true, "description": "Working directory" }
            },
            "required": ["command"]
        },
        "CommitParams": {
            "description": "Commit generation parameters",
            "type": "object",
            "properties": {
                "preview": { "type": "boolean", "description": "Preview mode" },
                "max_diff_size": { "type": "integer", "nullable": true, "description": "Maximum diff size" },
                "diff": { "type": "string", "nullable": true, "description": "Diff content" },
                "additional_context": { "type": "string", "nullable": true, "description": "Additional context" }
            }
        },
        "GenerateDataParams": {
            "description": "Data generation parameters",
            "type": "object",
            "properties": {
                "input": { "type": "string", "description": "Path to input JSONL file" },
                "schema": { "type": "string", "description": "Path to JSON schema file" },
                "system_prompt": { "type": "string", "nullable": true, "description": "Path to system prompt template" },
                "user_prompt": { "type": "string", "nullable": true, "description": "Path to user prompt template" },
                "concurrency": { "type": "integer", "nullable": true, "description": "Max concurrent requests" }
            },
            "required": ["input", "schema"]
        },
        "StreamMessage": {
            "description": "Streaming message",
            "type": "object",
            "properties": {
                "type": { "type": "string", "enum": ["chunk", "error", "complete"], "description": "Message type" },
                "data": { "type": "object", "nullable": true, "description": "Message data" },
                "message": { "type": "string", "nullable": true, "description": "Error message" }
            }
        }
    })
}

/// Returns the OpenRPC schema following the OpenRPC specification
/// https://spec.open-rpc.org/
/// Uses schemars to generate schemas from Rust types
fn get_openrpc_schema() -> Value {
    use schemars::schema_for;

    // Generate schemas from Rust types using schemars
    let schemas = json!({
        "ChatParams": schema_for!(crate::types::ChatParams),
        "ConversationParams": schema_for!(crate::types::ConversationParams),
        "RenameConversationParams": schema_for!(crate::types::RenameConversationParams),
        "ShellCommandParams": schema_for!(crate::types::ShellCommandParams),
        "WorkspacePathParams": schema_for!(crate::types::WorkspacePathParams),
        "SyncWorkspaceParams": schema_for!(crate::types::SyncWorkspaceParams),
        "QueryWorkspaceParams": schema_for!(crate::types::QueryWorkspaceParams),
        "ConfigParams": schema_for!(crate::types::ConfigParams),
        "McpConfigParams": schema_for!(crate::types::McpConfigParams),
        "ProviderAuthParams": schema_for!(crate::types::ProviderAuthParams),
        "SetActiveAgentParams": schema_for!(crate::types::SetActiveAgentParams),
        "CommitParams": schema_for!(crate::types::CommitParams),
        "CompactConversationParams": schema_for!(crate::types::CompactConversationParams),
        "GenerateCommandParams": schema_for!(crate::types::GenerateCommandParams),
        "GenerateDataParams": schema_for!(crate::types::GenerateDataParams),
        "DeleteWorkspacesParams": schema_for!(crate::types::DeleteWorkspacesParams),
        "McpAuthParams": schema_for!(crate::types::McpAuthParams),
        "McpLogoutParams": schema_for!(crate::types::McpLogoutParams),
        "McpAuthStatusParams": schema_for!(crate::types::McpAuthStatusParams),
        "WriteMcpConfigParams": schema_for!(crate::types::WriteMcpConfigParams),
        "InitWorkspaceParams": schema_for!(crate::types::InitWorkspaceParams),
        "ModelResponse": schema_for!(crate::types::ModelResponse),
        "AgentResponse": schema_for!(crate::types::AgentResponse),
        "FileResponse": schema_for!(crate::types::FileResponse),
        "ConversationResponse": schema_for!(crate::types::ConversationResponse),
        "CommandOutputResponse": schema_for!(crate::types::CommandOutputResponse),
        "WorkspaceInfoResponse": schema_for!(crate::types::WorkspaceInfoResponse),
        "FileStatusResponse": schema_for!(crate::types::FileStatusResponse),
        "CompactionResultResponse": schema_for!(crate::types::CompactionResultResponse),
        "ProviderResponse": schema_for!(crate::types::ProviderResponse),
        "UserInfoResponse": schema_for!(crate::types::UserInfoResponse),
        "UserUsageResponse": schema_for!(crate::types::UserUsageResponse),
        "CommandResponse": schema_for!(crate::types::CommandResponse),
        "SkillResponse": schema_for!(crate::types::SkillResponse),
        "CommitResultResponse": schema_for!(crate::types::CommitResultResponse),
        "AuthContextRequestResponse": schema_for!(crate::types::AuthContextRequestResponse),
        "NodeResponse": schema_for!(crate::types::NodeResponse),
        "SyncProgressResponse": schema_for!(crate::types::SyncProgressResponse),
        "ToolsOverviewResponse": schema_for!(crate::types::ToolsOverviewResponse),
        "ProviderModelsResponse": schema_for!(crate::types::ProviderModelsResponse),
        "StreamMessage": schema_for!(crate::types::StreamMessage),
    });

    // Build methods using the manually defined list but reference the derived
    // schemas
    let methods = get_all_jsonrpc_methods();
    let openrpc_methods: Vec<Value> = methods
        .iter()
        .map(|m| {
            let name = m.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let desc = m.get("description").and_then(|d| d.as_str()).unwrap_or("");
            let returns = m
                .get("returns")
                .cloned()
                .unwrap_or(json!({"type": "object"}));

            // Use derived schema for params if available
            let params = m.get("params").cloned().unwrap_or(json!(null));
            let params_schema = if params.is_null() {
                json!([])
            } else {
                json!([{
                    "name": "params",
                    "schema": params,
                    "required": true
                }])
            };

            json!({
                "name": name,
                "description": desc,
                "params": params_schema,
                "result": {
                    "name": "result",
                    "schema": returns
                }
            })
        })
        .collect();

    serde_json::json!({
        "openrpc": "1.0.0",
        "info": {
            "title": "Forge JSON-RPC API",
            "version": "1.0.0",
            "description": "JSON-RPC API for Forge - AI-powered coding assistant"
        },
        "methods": openrpc_methods,
        "components": {
            "schemas": schemas
        }
    })
}
