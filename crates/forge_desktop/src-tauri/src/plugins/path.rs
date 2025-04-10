use std::path::PathBuf;

use tauri::plugin::{Builder, TauriPlugin};
use tauri::Runtime;

#[tauri::command]
async fn get_home_dir() -> Result<Option<String>, String> {
    Ok(dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
async fn get_config_dir() -> Result<Option<String>, String> {
    Ok(dirs::config_dir().map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
async fn join_paths(base: &str, components: Vec<&str>) -> Result<String, String> {
    let mut path = PathBuf::from(base);
    for component in components {
        path = path.join(component);
    }
    Ok(path.to_string_lossy().to_string())
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("path")
        .invoke_handler(tauri::generate_handler![
            get_home_dir,
            get_config_dir,
            join_paths
        ])
        .build()
}
