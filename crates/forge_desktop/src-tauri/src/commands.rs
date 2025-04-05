use anyhow::Result;
use forge_api::{
    AgentMessage, ChatRequest, ChatResponse, ConversationId, Event, File, ModelId, ToolDefinition, API
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio_stream::StreamExt;

// Event type constants, matching those in the CLI UI
pub const EVENT_USER_TASK_INIT: &str = "user_task_init";
pub const EVENT_USER_TASK_UPDATE: &str = "user_task_update";
pub const EVENT_USER_HELP_QUERY: &str = "user_help_query";
pub const EVENT_TITLE: &str = "title";

// Define all the command payloads and responses
#[derive(Clone, Serialize)]
pub struct ModeInfo {
    pub mode: String,
    pub description: String,
}

#[derive(Clone, Serialize)]
pub struct ConversationInfo {
    pub id: String,
    pub title: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct SendMessageOptions {
    pub content: String,
    pub is_first: bool,
}

#[derive(Clone, Deserialize)]
pub struct ExportOptions {
    pub path: Option<String>,
    pub title: Option<String>,
}

// State for tracking conversation and mode
pub struct ForgeState {
    current_conversation_id: Mutex<Option<ConversationId>>,
    current_mode: Mutex<String>,
    is_first_message: Mutex<bool>,
}

impl ForgeState {
    pub fn new() -> Self {
        Self {
            current_conversation_id: Mutex::new(None),
            current_mode: Mutex::new("Act".to_string()),
            is_first_message: Mutex::new(true),
        }
    }
}

// Helper to access the API from commands
async fn get_api_and_state(app_handle: &AppHandle) -> (Arc<dyn API>, Arc<ForgeState>) {
    let api = app_handle.state::<Arc<dyn API>>();
    let state = app_handle.state::<Arc<ForgeState>>();
    (api.inner().clone(), state.inner().clone())
}

// Commands for interacting with the Forge API

/// Initialize a new conversation with the currently loaded workflow
#[tauri::command]
pub async fn init_conversation(app_handle: AppHandle) -> Result<String, String> {
    let (api, state) = get_api_and_state(&app_handle).await;
    
    // Load the workflow
    let workflow = api
        .load(None)
        .await
        .map_err(|e| format!("Failed to load workflow: {}", e))?;

    // Initialize the conversation
    let conversation_id = api
        .init(workflow)
        .await
        .map_err(|e| format!("Failed to initialize conversation: {}", e))?;
    
    // Store the conversation ID
    {
        let mut current_id = state.current_conversation_id.lock().await;
        *current_id = Some(conversation_id.clone());
    }
    
    // Set the mode variable in the conversation
    {
        let mode = state.current_mode.lock().await.clone();
        api.set_variable(&conversation_id, "mode".to_string(), Value::from(mode))
            .await
            .map_err(|e| format!("Failed to set mode variable: {}", e))?;
    }
    
    // Reset first message flag
    {
        let mut is_first = state.is_first_message.lock().await;
        *is_first = true;
    }

    Ok(conversation_id.to_string())
}

/// Load a workflow from a specific path
#[tauri::command]
pub async fn load_workflow(
    path: Option<String>,
    app_handle: AppHandle,
) -> Result<bool, String> {
    let (api, _) = get_api_and_state(&app_handle).await;
    
    let path_buf = path.map(PathBuf::from);
    let path_ref = path_buf.as_deref();

    api.load(path_ref)
        .await
        .map(|_| true)
        .map_err(|e| format!("Failed to load workflow: {}", e))
}

/// Send a message to the current conversation
#[tauri::command]
pub async fn send_message(
    options: SendMessageOptions,
    app_handle: AppHandle,
) -> Result<bool, String> {
    let (api, state) = get_api_and_state(&app_handle).await;
    
    // Get the current conversation ID
    let conversation_id = {
        let current_id = state.current_conversation_id.lock().await;
        match &*current_id {
            Some(id) => id.clone(),
            None => {
                // If no conversation exists, create one first
                drop(current_id); // Release the lock before calling init_conversation
                init_conversation(app_handle.clone()).await?;
                state.current_conversation_id.lock().await.clone().unwrap()
            }
        }
    };
    
    // Determine if this is the first message
    let is_first = if options.is_first {
        true
    } else {
        let is_first = state.is_first_message.lock().await;
        *is_first
    };
    
    // Update first message flag
    if is_first {
        let mut first_msg_flag = state.is_first_message.lock().await;
        *first_msg_flag = false;
    }
    
    // Get the current mode
    let mode = state.current_mode.lock().await.clone();
    
    // Create the appropriate event based on the mode and whether this is the first message
    let event = match mode.as_str() {
        "Help" => Event::new(EVENT_USER_HELP_QUERY, options.content),
        _ => {
            if is_first {
                Event::new(EVENT_USER_TASK_INIT, options.content)
            } else {
                Event::new(EVENT_USER_TASK_UPDATE, options.content)
            }
        }
    };

    // Create the chat request with the event
    let chat = ChatRequest::new(event, conversation_id);

    // Stream the response
    let mut stream = api.chat(chat).await
        .map_err(|e| format!("Failed to send message: {}", e))?;
    
    // Process the stream in a separate task
    let app_handle_clone = app_handle.clone();
    tokio::spawn(async move {
        while let Some(message_result) = stream.next().await {
            match message_result {
                Ok(agent_message) => {
                    // Emit the agent message to the frontend
                    let _ = app_handle_clone.emit("agent-message", &agent_message);
                    
                    // If this is a title event, update the conversation title
                    if let AgentMessage { message: ChatResponse::Event(event), .. } = &agent_message {
                        if event.name == EVENT_TITLE {
                            if let Some(title) = event.value.as_str() {
                                if let Some(conv_id) = state.current_conversation_id.lock().await.as_ref() {
                                    let _ = api.set_variable(conv_id, "title".to_string(), Value::from(title)).await;
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    // Emit error to the frontend
                    let _ = app_handle_clone.emit("agent-error", err.to_string());
                }
            }
        }
        // Signal that the stream is complete
        let _ = app_handle_clone.emit("agent-stream-complete", ());
    });
    
    Ok(true)
}

/// Change the current mode (Act, Plan, Help)
#[tauri::command]
pub async fn change_mode(mode: String, app_handle: AppHandle) -> Result<ModeInfo, String> {
    let (api, state) = get_api_and_state(&app_handle).await;
    
    let mode_info = ModeInfo {
        mode: mode.clone(),
        description: match mode.as_str() {
            "Act" => "mode - executes commands and makes file changes".to_string(),
            "Plan" => "mode - plans actions without making changes".to_string(),
            "Help" => "mode - answers questions (type /act or /plan to switch back)".to_string(),
            _ => format!("Unknown mode: {}", mode),
        },
    };
    
    // Update the mode
    {
        let mut current_mode = state.current_mode.lock().await;
        *current_mode = mode.clone();
    }

    // Set the mode in the conversation if one exists
    if let Some(conversation_id) = state.current_conversation_id.lock().await.as_ref() {
        api.set_variable(
                conversation_id,
                "mode".to_string(),
                Value::from(mode),
            )
            .await
            .map_err(|e| format!("Failed to set mode in conversation: {}", e))?;
    }

    Ok(mode_info)
}

/// Get the current mode
#[tauri::command]
pub async fn get_mode(app_handle: AppHandle) -> ModeInfo {
    let (_, state) = get_api_and_state(&app_handle).await;
    
    let mode = state.current_mode.lock().await.clone();
    ModeInfo {
        mode: mode.clone(),
        description: match mode.as_str() {
            "Act" => "mode - executes commands and makes file changes".to_string(),
            "Plan" => "mode - plans actions without making changes".to_string(),
            "Help" => "mode - answers questions (type /act or /plan to switch back)".to_string(),
            _ => format!("Unknown mode: {}", mode),
        },
    }
}

/// Get information about the current conversation
#[tauri::command]
pub async fn get_conversation_info(
    app_handle: AppHandle,
) -> Result<Option<ConversationInfo>, String> {
    let (api, state) = get_api_and_state(&app_handle).await;
    
    let conversation_id = state.current_conversation_id.lock().await.clone();
    
    match conversation_id {
        Some(id) => {
            let title = api
                .get_variable(&id, "title")
                .await
                .map_err(|e| format!("Failed to get conversation title: {}", e))?
                .and_then(|v| v.as_str().map(|s| s.to_string()));

            Ok(Some(ConversationInfo {
                id: id.to_string(),
                title,
            }))
        }
        None => Ok(None),
    }
}

/// Get available models
#[tauri::command]
pub async fn get_models(app_handle: AppHandle) -> Result<Vec<ModelId>, String> {
    let (api, _) = get_api_and_state(&app_handle).await;
    
    api.models()
        .await
        .map(|models| models.into_iter().map(|m| m.id).collect())
        .map_err(|e| format!("Failed to get models: {}", e))
}

/// Get file suggestions for autocomplete
#[tauri::command]
pub async fn get_suggestions(app_handle: AppHandle) -> Result<Vec<File>, String> {
    let (api, _) = get_api_and_state(&app_handle).await;
    
    api.suggestions()
        .await
        .map_err(|e| format!("Failed to get suggestions: {}", e))
}

/// Set a conversation variable
#[tauri::command]
pub async fn set_variable(
    key: String,
    value: Value,
    app_handle: AppHandle,
) -> Result<bool, String> {
    let (api, state) = get_api_and_state(&app_handle).await;
    
    let conversation_id = state.current_conversation_id.lock().await.clone();
    
    match conversation_id {
        Some(id) => {
            api.set_variable(&id, key, value)
                .await
                .map(|_| true)
                .map_err(|e| format!("Failed to set variable: {}", e))
        }
        None => Err("No active conversation".to_string()),
    }
}

/// Get environment information
#[tauri::command]
pub async fn get_environment(app_handle: AppHandle) -> Value {
    let (api, _) = get_api_and_state(&app_handle).await;
    
    let env = api.environment();
    serde_json::to_value(env).unwrap_or_default()
}

/// Export the current conversation to a file
#[tauri::command]
pub async fn export_conversation(
    options: ExportOptions,
    app_handle: AppHandle,
) -> Result<String, String> {
    let (api, state) = get_api_and_state(&app_handle).await;
    
    // Check if we have an active conversation
    let conversation_id = match state.current_conversation_id.lock().await.clone() {
        Some(id) => id,
        None => return Err("No active conversation to export".to_string()),
    };
    
    // Get the conversation
    let conversation = api.conversation(&conversation_id).await
        .map_err(|e| format!("Failed to get conversation: {}", e))?
        .ok_or_else(|| "Conversation not found".to_string())?;
    
    // Generate a timestamp for the filename
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    
    // Use the provided title or get it from the conversation
    let title = match options.title {
        Some(t) => t,
        None => {
            api.get_variable(&conversation_id, "title")
                .await
                .map_err(|e| format!("Failed to get conversation title: {}", e))?
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "untitled".to_string())
        }
    };
    
    // Generate the filename
    let filename = format!("{}-{}-dump.json", timestamp, title);
    
    // Determine the path to save to
    let path = match options.path {
        Some(p) => PathBuf::from(p).join(&filename),
        None => app_handle
            .path()
            .document_dir()
            .map_err(|_| "Failed to get document directory".to_string())?
            .join(&filename),
    };
    
    // Export the conversation
    let content = serde_json::to_string_pretty(&conversation)
        .map_err(|e| format!("Failed to serialize conversation: {}", e))?;
        
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Failed to write file: {}", e))?;
    
    Ok(path.to_string_lossy().to_string())
}

/// Get available tools
#[tauri::command]
pub async fn get_tools(app_handle: AppHandle) -> Vec<ToolDefinition> {
    let (api, _) = get_api_and_state(&app_handle).await;
    
    api.tools()
        .await
}