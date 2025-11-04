use std::path::PathBuf;

use uuid::Uuid;

const CLIENT_ID_FILE: &str = ".forge_client_id";

/// Gets or creates a persistent client ID for Android
pub fn get_or_create_client_id() -> anyhow::Result<String> {
    let home_dir = dirs::home_dir().unwrap_or(PathBuf::from("."));

    let client_id_path: PathBuf = home_dir.join(CLIENT_ID_FILE);

    if let Ok(existing_id) = std::fs::read_to_string(&client_id_path) {
        let trimmed = existing_id.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let new_id = Uuid::new_v4().to_string();
    std::fs::write(&client_id_path, &new_id)?;

    Ok(new_id)
}
