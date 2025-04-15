//! JSON-RPC method handlers implementation

use std::path::PathBuf;
use std::sync::Arc;

use forge_api::ForgeAPI;
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_services::ForgeServices;
use jsonrpc_core::{Error as JsonRpcError, ErrorCode, IoHandler, Params, Value};
use tracing::{debug, info};

use crate::error::{error_codes, map_error, string_to_error};
use crate::stream_handler::StreamManager;

/// Registers all method handlers with the JSON-RPC server
pub fn register_methods(
    io: &mut IoHandler,
    api: Arc<ForgeAPI<ForgeServices<ForgeInfra>>>,
    stream_manager: Arc<StreamManager>,
) {
    info!("Registering JSON-RPC method handlers");

    // suggestions
    let api_clone = api.clone();
    io.add_method("suggestions", move |_params| {
        let api = api_clone.clone();
        debug!("Method call: suggestions");

        async move {
            let suggestions = api.suggestions().await.map_err(map_error)?;
            Ok(serde_json::to_value(suggestions).unwrap())
        }
    });

    // tools
    let api_clone = api.clone();
    io.add_method("tools", move |_params| {
        let api = api_clone.clone();
        debug!("Method call: tools");

        async move {
            let tools = api.tools().await;
            Ok(serde_json::to_value(tools).unwrap())
        }
    });

    // models
    let api_clone = api.clone();
    io.add_method("models", move |_params| {
        let api = api_clone.clone();
        debug!("Method call: models");

        async move {
            let models = api.models().await.map_err(map_error)?;
            Ok(serde_json::to_value(models).unwrap())
        }
    });

    // environment
    let api_clone = api.clone();
    io.add_method("environment", move |_params| {
        let api = api_clone.clone();
        debug!("Method call: environment");

        async move {
            let environment = api.environment();
            Ok(serde_json::to_value(environment).unwrap())
        }
    });

    // init
    let api_clone = api.clone();
    io.add_method("init", move |params: Params| {
        let api = api_clone.clone();
        debug!("Method call: init with params: {:?}", params);

        async move {
            let workflow: Option<Workflow> = params.parse().map_err(|e| JsonRpcError {
                code: ErrorCode::InvalidParams,
                message: format!("Invalid workflow parameter: {}", e),
                data: None,
            })?;

            let conversation_id = api
                .init(workflow.unwrap_or_default())
                .await
                .map_err(map_error)?;
            Ok(serde_json::to_value(conversation_id).unwrap())
        }
    });

    // load
    let api_clone = api.clone();
    io.add_method("load", move |params: Params| {
        let api = api_clone.clone();
        debug!("Method call: load with params: {:?}", params);

        async move {
            let path_str: Option<String> = params.parse().map_err(|e| JsonRpcError {
                code: ErrorCode::InvalidParams,
                message: format!("Invalid path parameter: {}", e),
                data: None,
            })?;

            // Convert to PathBuf and get a reference to Path
            let path_opt = path_str.map(PathBuf::from);
            let path_ref = path_opt.as_deref();

            let workflow = api.load(path_ref).await.map_err(map_error)?;
            Ok(serde_json::to_value(workflow).unwrap())
        }
    });

    // conversation
    let api_clone = api.clone();
    io.add_method("conversation", move |params: Params| {
        let api = api_clone.clone();
        debug!("Method call: conversation with params: {:?}", params);

        async move {
            let conversation_id: ConversationId = params.parse().map_err(|e| JsonRpcError {
                code: ErrorCode::InvalidParams,
                message: format!("Invalid conversation_id parameter: {}", e),
                data: None,
            })?;

            let conversation = api
                .conversation(&conversation_id)
                .await
                .map_err(map_error)?;
            Ok(serde_json::to_value(conversation).unwrap())
        }
    });

    // upsert_conversation
    let api_clone = api.clone();
    io.add_method("upsertConversation", move |params: Params| {
        let api = api_clone.clone();
        debug!("Method call: upsertConversation with params: {:?}", params);

        async move {
            let conversation: Conversation = params.parse().map_err(|e| JsonRpcError {
                code: ErrorCode::InvalidParams,
                message: format!("Invalid conversation parameter: {}", e),
                data: None,
            })?;

            api.upsert_conversation(conversation)
                .await
                .map_err(map_error)?;
            Ok(Value::Null)
        }
    });

    // get_variable
    let api_clone = api.clone();
    io.add_method("getVariable", move |params: Params| {
        let api = api_clone.clone();
        debug!("Method call: getVariable with params: {:?}", params);

        async move {
            #[derive(serde::Deserialize)]
            struct GetVarParams {
                conversation_id: ConversationId,
                key: String,
            }

            let params: GetVarParams = params.parse().map_err(|e| JsonRpcError {
                code: ErrorCode::InvalidParams,
                message: format!("Invalid getVariable parameters: {}", e),
                data: None,
            })?;

            let value = api
                .get_variable(&params.conversation_id, &params.key)
                .await
                .map_err(map_error)?;

            Ok(serde_json::to_value(value).unwrap())
        }
    });

    // set_variable
    let api_clone = api.clone();
    io.add_method("setVariable", move |params: Params| {
        let api = api_clone.clone();
        debug!("Method call: setVariable with params: {:?}", params);

        async move {
            #[derive(serde::Deserialize)]
            struct SetVarParams {
                conversation_id: ConversationId,
                key: String,
                value: Value,
            }

            let params: SetVarParams = params.parse().map_err(|e| JsonRpcError {
                code: ErrorCode::InvalidParams,
                message: format!("Invalid setVariable parameters: {}", e),
                data: None,
            })?;

            api.set_variable(&params.conversation_id, params.key, params.value)
                .await
                .map_err(map_error)?;

            Ok(Value::Bool(true))
        }
    });

    // chat - special handling for streaming response
    let api_clone = api.clone();
    let stream_manager_clone = stream_manager.clone();
    io.add_method("chat", move |params: Params| {
        let api = api_clone.clone();
        let stream_manager = stream_manager_clone.clone();
        debug!("Method call: chat with params: {:?}", params);

        async move {
            let chat_request: ChatRequest = params.parse().map_err(|e| JsonRpcError {
                code: ErrorCode::InvalidParams,
                message: format!("Invalid chat_request parameter: {}", e),
                data: None,
            })?;

            let stream = api.chat(chat_request).await.map_err(map_error)?;
            let stream_id = stream_manager.register_stream(stream).await;

            Ok(serde_json::to_value(stream_id).unwrap())
        }
    });

    // cancelChat - cancel a chat stream
    let stream_manager_clone = stream_manager.clone();
    io.add_method("cancelChat", move |params: Params| {
        let stream_manager = stream_manager_clone.clone();
        debug!("Method call: cancelChat with params: {:?}", params);

        async move {
            let stream_id: String = params.parse().map_err(|e| JsonRpcError {
                code: ErrorCode::InvalidParams,
                message: format!("Invalid stream_id parameter: {}", e),
                data: None,
            })?;

            let removed = stream_manager.remove_stream(&stream_id);

            if removed {
                Ok(Value::Bool(true))
            } else {
                Err(string_to_error(
                    &format!("Stream not found: {}", stream_id),
                    error_codes::INVALID_STREAM,
                ))
            }
        }
    });
}
