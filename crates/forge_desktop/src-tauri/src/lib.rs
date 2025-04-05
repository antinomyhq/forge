mod commands;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::init_conversation,
            commands::load_workflow,
            commands::send_message,
            commands::change_mode,
            commands::get_mode,
            commands::get_conversation_info,
            commands::get_models,
            commands::get_suggestions,
            commands::set_variable,
            commands::get_environment,
            commands::export_conversation,
            commands::get_tools,
        ])
        .setup(|app| {
            // Initialize the ForgeAPI directly, similar to the CLI application
            use forge_api::ForgeAPI;
            use std::sync::Arc;
            
            // Initialize with restricted mode set to false by default
            // This matches how the CLI initializes it when the flag isn't passed
            let api = Arc::new(ForgeAPI::init(false)) as Arc<dyn forge_api::API>;
            
            // Create the state tracker for conversation and mode
            let state = Arc::new(commands::ForgeState::new());
            
            // Manage the API and state
            app.manage(api);
            app.manage(state);
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

