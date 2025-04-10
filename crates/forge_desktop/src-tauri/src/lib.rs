mod commands;
mod file_commands;
mod plugins;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Try to load environment variables before starting the app
    if let Ok(loaded) = dotenv::dotenv() {
        println!("Loaded environment from: {}", loaded.display());
    }

    // Try loading from home directory if default fails
    if let Some(home_dir) = dirs::home_dir() {
        let env_paths = vec![
            home_dir.join(".env"),
            home_dir.join(".forge").join(".env"),
            home_dir.join(".config").join("forge").join(".env"),
        ];

        for path in env_paths {
            if path.exists() && dotenv::from_path(&path).is_ok() {
                break;
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(plugins::init_path())
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
            commands::load_projects_state,
            commands::save_projects_state,
            commands::select_project,
            commands::get_current_project,
            commands::get_recent_projects,
            commands::close_current_project,
            commands::switch_project,
            commands::create_new_project,
            commands::update_cwd,
            commands::cancel_stream,
            commands::get_directory_structure,
            // File viewer commands
            file_commands::read_file_content,
            file_commands::get_file_info,
            file_commands::file_exists,
        ])
        .setup(|app| {
            // Initialize the ForgeAPI directly, similar to the CLI application
            use std::sync::Arc;

            use forge_api::ForgeAPI;

            // Initialize with restricted mode set to false by default
            // This matches how the CLI initializes it when the flag isn't passed
            let api = Arc::new(ForgeAPI::init(false)) as Arc<dyn forge_api::API>;

            // Create the state tracker for conversation and mode
            let state = Arc::new(commands::ForgeState::new());

            // Manage the API and state
            app.manage(api);
            app.manage(state);

            // Create projects directory
            if let Ok(app_dir) = app.path().app_config_dir() {
                let projects_dir = app_dir.join("projects");
                let _ = std::fs::create_dir_all(&projects_dir);
            }

            // Load project state at startup
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = commands::load_projects_state(app_handle).await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
