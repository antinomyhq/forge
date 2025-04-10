use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use chrono::Local;
use forge_api::{
    AgentMessage, ChatRequest, ChatResponse, ConversationId, Event, File, ModelId, ToolDefinition,
    API,
};
use forge_walker::Walker;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

// Event type constants, matching those in the CLI UI
pub const EVENT_USER_TASK_INIT: &str = "user_task_init";
pub const EVENT_USER_TASK_UPDATE: &str = "user_task_update";
pub const EVENT_USER_HELP_QUERY: &str = "user_help_query";
pub const EVENT_TITLE: &str = "title";

// Define all the command payloads and responses
// Project information data structure
#[derive(Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub path: String,
    pub name: String,
    pub last_accessed: String, // ISO timestamp
}

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
    current_stream_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,

    // Project management state
    current_project: Mutex<Option<ProjectInfo>>,
    recent_projects: Mutex<VecDeque<ProjectInfo>>,
    max_recent_projects: usize,
}

impl ForgeState {
    pub fn new() -> Self {
        Self {
            current_project: Mutex::new(None),
            recent_projects: Mutex::new(VecDeque::new()),
            max_recent_projects: 10,

            current_conversation_id: Mutex::new(None),
            current_mode: Mutex::new("ACT".to_string()),
            is_first_message: Mutex::new(true),
            current_stream_handle: Mutex::new(None),
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
pub async fn load_workflow(path: Option<String>, app_handle: AppHandle) -> Result<bool, String> {
    let (api, _) = get_api_and_state(&app_handle).await;

    let path_buf = path.map(PathBuf::from);
    let path_ref = path_buf.as_deref();

    api.load(path_ref)
        .await
        .map(|_| true)
        .map_err(|e| format!("Failed to load workflow: {}", e))
}

/// Helper to handle a single chat response message
async fn handle_chat_response(
    message: AgentMessage<ChatResponse>,
    app_handle: &AppHandle,
    state: &Arc<ForgeState>,
    api: &Arc<dyn API>,
) -> Result<(), String> {
    // Simply emit the message to the frontend without any transformations
    let _ = app_handle.emit("agent-message", &message);

    // Special handling for title events - this is business logic, not
    // transformation
    if let AgentMessage { message: ChatResponse::Event(event), .. } = &message {
        if event.name == EVENT_TITLE {
            if let Some(title) = event.value.as_str() {
                if let Some(conv_id) = state.current_conversation_id.lock().await.as_ref() {
                    let _ = api
                        .set_variable(conv_id, "title".to_string(), Value::from(title))
                        .await;
                }
            }
        }
    }

    Ok(())
}

/// Process an agent message stream, handling each message appropriately
async fn handle_chat_stream(
    stream: impl futures_util::StreamExt<Item = Result<AgentMessage<ChatResponse>, anyhow::Error>>
        + Unpin,
    app_handle: AppHandle,
    state: Arc<ForgeState>,
    api: Arc<dyn API>,
) -> Result<(), String> {
    // Create a shared HashSet for tracking processed messages
    let processed_msgs = Arc::new(Mutex::new(std::collections::HashSet::new()));

    tokio::pin!(stream);

    // Process the stream directly without complex transformations
    while let Some(message_result) = stream.next().await {
        match message_result {
            Ok(agent_message) => {
                // Generate a unique ID for the message to detect duplicates
                let msg_id = format!("{:?}-{:?}", agent_message.agent, agent_message.message);

                // Only process if we haven't seen this message before
                let should_process = {
                    let mut processed = processed_msgs.lock().await;
                    processed.insert(msg_id)
                };

                if should_process {
                    // Call the response handler
                    if let Err(err) =
                        handle_chat_response(agent_message, &app_handle, &state, &api).await
                    {
                        eprintln!("Error processing message: {}", err);
                    }
                }
            }
            Err(err) => {
                // Emit error to the frontend
                let _ = app_handle.emit("agent-error", err.to_string());
                eprintln!("Stream error: {}", err);
            }
        }
    }

    // Signal that the stream is complete
    let _ = app_handle.emit("agent-stream-complete", ());

    Ok(())
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

    // Create the appropriate event based on the mode and whether this is the first
    // message
    let event = match mode.as_str() {
        "HELP" => Event::new(EVENT_USER_HELP_QUERY, options.content),
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
    let stream = api
        .chat(chat)
        .await
        .map_err(|e| format!("Failed to send message: {}", e))?;

    // Setup a cancellable task to process the stream
    let app_handle_clone = app_handle.clone();
    let api_clone = api.clone();
    let state_clone = state.clone();

    let stream_handle = tokio::spawn(async move {
        // Handle the stream in a simpler way, similar to ui.rs
        if let Err(e) = handle_chat_stream(stream, app_handle_clone, state_clone, api_clone).await {
            eprintln!("Error handling chat stream: {}", e);
        }
    });

    // Store the stream handle for potential cancellation
    {
        let mut current_stream = state.current_stream_handle.lock().await;
        *current_stream = Some(stream_handle);
    }

    Ok(true)
}

/// Change the current mode (Act, Plan, Help)
#[tauri::command]
pub async fn change_mode(mode: String, app_handle: AppHandle) -> Result<ModeInfo, String> {
    let (api, state) = get_api_and_state(&app_handle).await;

    // Convert display mode (Act, Plan, Help) to variable mode (ACT, PLAN, HELP)
    let api_mode = mode.to_uppercase();

    // Display mode matches what the UI expects
    let display_mode = mode.clone();

    let mode_info = ModeInfo {
        mode: display_mode.clone(),
        description: match display_mode.as_str() {
            "Act" => "mode - executes commands and makes file changes".to_string(),
            "Plan" => "mode - plans actions without making changes".to_string(),
            "Help" => "mode - answers questions (type /act or /plan to switch back)".to_string(),
            _ => format!("Unknown mode: {}", display_mode),
        },
    };

    // Update the mode (store uppercase for API)
    {
        let mut current_mode = state.current_mode.lock().await;
        *current_mode = api_mode.clone();
    }

    // Set the mode in the conversation if one exists (use uppercase for API)
    if let Some(conversation_id) = state.current_conversation_id.lock().await.as_ref() {
        api.set_variable(conversation_id, "mode".to_string(), Value::from(api_mode))
            .await
            .map_err(|e| format!("Failed to set mode in conversation: {}", e))?;
    }

    Ok(mode_info)
}

/// Get the current mode
#[tauri::command]
pub async fn get_mode(app_handle: AppHandle) -> ModeInfo {
    let (_, state) = get_api_and_state(&app_handle).await;

    let api_mode = state.current_mode.lock().await.clone();

    // Convert API mode (uppercase) to display mode (title case) for UI
    let display_mode = match api_mode.as_str() {
        "ACT" => "Act",
        "PLAN" => "Plan",
        "HELP" => "Help",
        _ => &api_mode, // Fallback to original if unknown
    }
    .to_string();

    ModeInfo {
        mode: display_mode.clone(),
        description: match display_mode.as_str() {
            "Act" => "mode - executes commands and makes file changes".to_string(),
            "Plan" => "mode - plans actions without making changes".to_string(),
            "Help" => "mode - answers questions (type /act or /plan to switch back)".to_string(),
            _ => format!("Unknown mode: {}", display_mode),
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

            Ok(Some(ConversationInfo { id: id.to_string(), title }))
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
        Some(id) => api
            .set_variable(&id, key, value)
            .await
            .map(|_| true)
            .map_err(|e| format!("Failed to set variable: {}", e)),
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
    let conversation = api
        .conversation(&conversation_id)
        .await
        .map_err(|e| format!("Failed to get conversation: {}", e))?
        .ok_or_else(|| "Conversation not found".to_string())?;

    // Generate a timestamp for the filename
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");

    // Use the provided title or get it from the conversation
    let title = match options.title {
        Some(t) => t,
        None => api
            .get_variable(&conversation_id, "title")
            .await
            .map_err(|e| format!("Failed to get conversation title: {}", e))?
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "untitled".to_string()),
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

    api.tools().await
}
/// Get project state file path
async fn get_projects_file_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let app_dir = app_handle
        .path()
        .app_config_dir()
        .map_err(|_| "Failed to get app config directory".to_string())?;

    let projects_dir = app_dir.join("projects");
    fs::create_dir_all(&projects_dir)
        .map_err(|e| format!("Failed to create projects directory: {}", e))?;

    Ok(projects_dir.join("recent_projects.json"))
}

/// Update recent projects list with a new project
async fn update_recent_projects(
    state: &Arc<ForgeState>,
    project_info: ProjectInfo,
) -> Result<(), String> {
    let mut recent_projects = state.recent_projects.lock().await;

    // Remove the project if it already exists in the list
    recent_projects.retain(|p| p.path != project_info.path);

    // Add the project to the front of the list
    recent_projects.push_front(project_info);

    // Keep only the maximum number of projects
    while recent_projects.len() > state.max_recent_projects {
        recent_projects.pop_back();
    }

    Ok(())
}

/// Save projects state to disk
#[tauri::command]
pub async fn save_projects_state(app_handle: AppHandle) -> Result<bool, String> {
    let (_, state) = get_api_and_state(&app_handle).await;

    let projects_file = get_projects_file_path(&app_handle).await?;

    // Get current project and recent projects
    let current_project = state.current_project.lock().await.clone();
    let recent_projects = state.recent_projects.lock().await.clone();

    // Create the state to save
    let save_state = serde_json::json!({
        "current_project": current_project,
        "recent_projects": recent_projects,
    });

    // Save to file
    let content = serde_json::to_string_pretty(&save_state)
        .map_err(|e| format!("Failed to serialize project state: {}", e))?;

    tokio::fs::write(&projects_file, content)
        .await
        .map_err(|e| format!("Failed to write project state file: {}", e))?;

    Ok(true)
}

/// Load projects state from disk
#[tauri::command]
pub async fn load_projects_state(app_handle: AppHandle) -> Result<bool, String> {
    let (_, state) = get_api_and_state(&app_handle).await;

    let projects_file = get_projects_file_path(&app_handle).await?;

    // Check if the file exists
    if !projects_file.exists() {
        return Ok(false); // No state to load
    }

    // Read the file
    let content = tokio::fs::read_to_string(&projects_file)
        .await
        .map_err(|e| format!("Failed to read project state file: {}", e))?;

    // Parse the JSON
    let save_state = serde_json::from_str::<serde_json::Value>(&content)
        .map_err(|e| format!("Failed to parse project state: {}", e))?;

    // Extract and update the state
    if let Some(current_project) = save_state["current_project"].as_object() {
        let project_info = serde_json::from_value::<ProjectInfo>(serde_json::Value::Object(
            current_project.clone(),
        ))
        .map_err(|e| format!("Failed to parse current project: {}", e))?;

        let mut current = state.current_project.lock().await;
        *current = Some(project_info);
    }

    if let Some(recent_projects) = save_state["recent_projects"].as_array() {
        let projects = recent_projects
            .iter()
            .filter_map(|v| serde_json::from_value::<ProjectInfo>(v.clone()).ok())
            .collect::<VecDeque<_>>();

        let mut recent = state.recent_projects.lock().await;
        *recent = projects;
    }

    Ok(true)
}

/// Select a project and set it as current
#[tauri::command]
pub async fn select_project(
    path: String,
    name: Option<String>,
    app_handle: AppHandle,
) -> Result<ProjectInfo, String> {
    let (_api, state) = get_api_and_state(&app_handle).await;

    // Validate the path
    let path_buf = PathBuf::from(&path);
    if !path_buf.is_dir() {
        return Err(format!("Path '{}' is not a valid directory", path));
    }

    // Determine the project name
    let name = name.unwrap_or_else(|| {
        path_buf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unnamed Project")
            .to_string()
    });

    // Create the project info
    let now = Local::now();
    let project_info = ProjectInfo { path: path.clone(), name, last_accessed: now.to_rfc3339() };

    // Update the current project
    {
        let mut current_project = state.current_project.lock().await;
        *current_project = Some(project_info.clone());
    }

    // Update the recent projects list
    update_recent_projects(&state, project_info.clone()).await?;

    // Save the state
    save_projects_state(app_handle.clone()).await?;

    // Load the workflow from the project directory
    let _ = load_workflow(Some(path.clone()), app_handle.clone()).await;

    // Update the environment's cwd to match the project path
    let _ = update_cwd(path.clone(), app_handle.clone()).await?;

    // Initialize a new conversation for this project
    let _ = init_conversation(app_handle).await?;

    Ok(project_info)
}

/// Get the current project
#[tauri::command]
pub async fn get_current_project(app_handle: AppHandle) -> Option<ProjectInfo> {
    let (_, state) = get_api_and_state(&app_handle).await;
    let current = state.current_project.lock().await;
    current.clone()
}

/// Get the list of recent projects
#[tauri::command]
pub async fn get_recent_projects(app_handle: AppHandle) -> Vec<ProjectInfo> {
    let (_, state) = get_api_and_state(&app_handle).await;
    let recent_projects = state.recent_projects.lock().await;
    recent_projects.clone().into_iter().collect()
}

/// Close the current project
#[tauri::command]
pub async fn close_current_project(app_handle: AppHandle) -> Result<bool, String> {
    let (_, state) = get_api_and_state(&app_handle).await;

    // Clear the current project
    {
        let mut current_project = state.current_project.lock().await;
        *current_project = None;
    }

    // Reset conversation state
    {
        let mut current_conversation_id = state.current_conversation_id.lock().await;
        *current_conversation_id = None;
    }

    // Reset first message flag
    {
        let mut is_first = state.is_first_message.lock().await;
        *is_first = true;
    }

    // Save the updated state
    save_projects_state(app_handle).await
}

/// Switch to a different project
#[tauri::command]
pub async fn switch_project(path: String, app_handle: AppHandle) -> Result<ProjectInfo, String> {
    let (_, state) = get_api_and_state(&app_handle).await;

    // Find the project in recent projects
    let project_info = {
        let recent_projects = state.recent_projects.lock().await;
        recent_projects.iter().find(|p| p.path == path).cloned()
    };

    // If found, select it
    if let Some(mut project) = project_info {
        // Update the last accessed time
        let now = Local::now();
        project.last_accessed = now.to_rfc3339();

        // Select the project
        return select_project(project.path, Some(project.name), app_handle).await;
    }

    // If not found, try to select it as a new project
    select_project(path, None, app_handle).await
}

/// Update the environment's current working directory
#[tauri::command]
pub async fn update_cwd(cwd: String, app_handle: AppHandle) -> Result<bool, String> {
    let (api, _) = get_api_and_state(&app_handle).await;

    let cwd_path = PathBuf::from(&cwd);

    // Validate the path
    if !cwd_path.is_dir() {
        return Err(format!("Path is not a valid directory: {}", cwd));
    }

    // Update the environment's cwd
    api.update_cwd(cwd_path)
        .map_err(|e| format!("Failed to update working directory: {}", e))
        .map(|_| true)
}

/// Create a new project
#[tauri::command]
pub async fn create_new_project(
    path: String,
    name: Option<String>,
    app_handle: AppHandle,
) -> Result<ProjectInfo, String> {
    // Validate the path
    let path_buf = PathBuf::from(&path);

    // Create the directory if it doesn't exist
    if !path_buf.exists() {
        fs::create_dir_all(&path_buf).map_err(|e| format!("Failed to create directory: {}", e))?;
    } else if !path_buf.is_dir() {
        return Err(format!("'{}' is not a directory", path));
    }

    // Initialize the directory as a project by creating a forge.yaml if it doesn't
    // exist
    let forge_yaml = path_buf.join("forge.yaml");
    if !forge_yaml.exists() {
        let default_content = "# Forge configuration\nworkflow: default\nproviders:\n  - type: openai\n    model: gpt-4-turbo-preview\n";

        fs::write(&forge_yaml, default_content)
            .map_err(|e| format!("Failed to create forge.yaml: {}", e))?;
    }

    // Select the newly created project
    select_project(path, name, app_handle).await
}
/// Cancel the current stream if one is active
#[tauri::command]
pub async fn cancel_stream(app_handle: AppHandle) -> Result<bool, String> {
    let (_, state) = get_api_and_state(&app_handle).await;

    // Get the current stream handle
    let stream_handle = {
        let mut current_stream = state.current_stream_handle.lock().await;
        current_stream.take()
    };

    // If there's an active stream, abort it
    if let Some(handle) = stream_handle {
        // Abort the task
        handle.abort();

        // Wait for the task to complete (it will be cancelled)
        match handle.await {
            Ok(_) => {
                // Task completed normally (unlikely after abort)
                let _ = app_handle.emit("agent-stream-complete", ());
            }
            Err(_) => {
                // Task was cancelled as expected
                let _ = app_handle.emit("agent-stream-complete", ());
            }
        }

        Ok(true)
    } else {
        // No active stream to cancel
        Ok(false)
    }
}

/// Represents a file or directory in the file system
#[derive(Clone, Serialize)]
pub struct FileSystemEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub children: Option<Vec<FileSystemEntry>>,
}

/// Get the directory structure of a project
#[tauri::command]
pub async fn get_directory_structure(
    path: String,
    depth: Option<u32>,
    _app_handle: AppHandle,
) -> Result<FileSystemEntry, String> {
    let path_buf = PathBuf::from(&path);

    // Validate the path
    if !path_buf.is_dir() {
        return Err(format!("Path is not a valid directory: {}", path));
    }

    // Create wrapper for root directory
    let root_name = path_buf
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Project")
        .to_string();

    // Using forge_walker to get files with gitignore respect
    let walker = Walker::max_all()
        .cwd(path_buf.clone())
        .max_depth(depth.unwrap_or(1024) as usize) // Default depth 5
        .max_breadth(1000) // Allow up to 1000 files per directory
        .skip_binary(false); // We want to show all files in the directory view

    let files = walker
        .get()
        .await
        .map_err(|e| format!("Failed to traverse directory: {}", e))?;

    // Root directory entry
    let mut root = FileSystemEntry {
        name: root_name,
        path: path_buf.to_string_lossy().to_string(),
        is_directory: true,
        children: Some(Vec::new()),
    };

    let mut dir_entries: std::collections::HashMap<String, Vec<FileSystemEntry>> =
        std::collections::HashMap::new();

    // Process files and build the directory structure
    for file in files {
        let rel_path = file.path.trim_end_matches('/');

        // Skip the root directory itself
        if rel_path.is_empty() {
            continue;
        }

        let is_dir = file.is_dir();
        let components: Vec<&str> = rel_path.split('/').collect();

        // Create FileSystemEntry for this file/directory
        let entry = FileSystemEntry {
            name: file
                .file_name
                .unwrap_or_else(|| components.last().unwrap_or(&"").to_string()),
            path: Path::new(&path_buf)
                .join(rel_path)
                .to_string_lossy()
                .to_string(),
            is_directory: is_dir,
            children: if is_dir { Some(Vec::new()) } else { None },
        };

        // Determine the parent path
        let parent_path = if components.len() > 1 {
            components[..components.len() - 1].join("/")
        } else {
            "".to_string() // Root level
        };

        // Add to the appropriate directory's children
        dir_entries.entry(parent_path).or_default().push(entry);
    }

    // Build the tree structure recursively
    fn build_tree(
        entry: &mut FileSystemEntry,
        dir_entries: &std::collections::HashMap<String, Vec<FileSystemEntry>>,
        path: &str,
    ) {
        if let Some(children) = &mut entry.children {
            if let Some(entries) = dir_entries.get(path) {
                // Sort: directories first, then alphabetical
                let mut sorted_entries = entries.clone();
                sorted_entries.sort_by(|a, b| match (a.is_directory, b.is_directory) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                });

                *children = sorted_entries.clone();

                // Recursively process subdirectories
                for child in children.iter_mut().filter(|c| c.is_directory) {
                    let child_path = if path.is_empty() {
                        child.name.clone()
                    } else {
                        format!("{}/{}", path, child.name)
                    };

                    build_tree(child, dir_entries, &child_path);
                }
            }
        }
    }

    // Build the tree starting from root
    build_tree(&mut root, &dir_entries, "");

    Ok(root)
}
