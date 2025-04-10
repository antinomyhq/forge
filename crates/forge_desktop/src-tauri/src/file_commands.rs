use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::command;

#[derive(Serialize, Deserialize)]
pub struct FileInfo {
    name: String,
    extension: String,
    size: u64,
    is_file: bool,
    is_directory: bool,
    is_symlink: bool,
    created_at: Option<u64>,
    modified_at: Option<u64>,
}

/// Read the content of a file as text
#[command]
pub fn read_file_content(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))
}

/// Read file metadata
#[command]
pub fn get_file_info(path: &str) -> Result<FileInfo, String> {
    let path_obj = Path::new(path);
    let metadata =
        fs::metadata(path).map_err(|e| format!("Failed to read file metadata: {}", e))?;

    let file_name = path_obj.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let extension = path_obj.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Convert timestamps to milliseconds since UNIX epoch
    // These might be None on some platforms
    let created_at = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() * 1000 + d.subsec_millis() as u64);

    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() * 1000 + d.subsec_millis() as u64);

    Ok(FileInfo {
        name: file_name.to_string(),
        extension: extension.to_string(),
        size: metadata.len(),
        is_file: metadata.is_file(),
        is_directory: metadata.is_dir(),
        is_symlink: metadata.is_symlink(),
        created_at,
        modified_at,
    })
}

/// Check if a file exists
#[command]
pub fn file_exists(path: &str) -> bool {
    Path::new(path).exists()
}
