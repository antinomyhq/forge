use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use dashmap::DashMap;

use crate::client::LspClient;
use crate::install::find_or_install;
use crate::language::get_language_id;
use crate::server::{find_root, get_server_definitions, ServerDefinition};

pub struct LspManager {
    clients: DashMap<String, Arc<LspClient>>, // Key: Root Path + Server ID
    server_definitions: Vec<ServerDefinition>,
}

impl LspManager {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            server_definitions: get_server_definitions(),
        }
    }

    pub async fn get_client(&self, file_path: &Path) -> Result<Arc<LspClient>> {
        let language_id =
            get_language_id(file_path).ok_or_else(|| anyhow!("Unsupported file extension"))?;

        let definition = self
            .server_definitions
            .iter()
            .find(|def| def.language_ids.iter().any(|id| id == language_id))
            .ok_or_else(|| anyhow!("No LSP server defined for language: {}", language_id))?;

        let root_path = find_root(file_path, &definition.root_markers)
            .ok_or_else(|| anyhow!("Could not determine project root"))?;

        let key = format!("{}::{}", root_path.display(), definition.id);

        if let Some(client) = self.clients.get(&key) {
            return Ok(client.clone());
        }

        // Ensure server is installed and get the executable path
        let executable_path =
            find_or_install(&definition.command, definition.installation.as_ref())?;
        let executable_str = executable_path.to_string_lossy().to_string();

        // Create new client
        let client = LspClient::new(&executable_str, &definition.args, &root_path).await?;
        let client = Arc::new(client);

        self.clients.insert(key, client.clone());
        Ok(client)
    }
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}
