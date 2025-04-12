use std::sync::Arc;

use forge_api::ForgeAPI;

/// State shared between handlers
pub struct AppState<F> {
    pub api: Arc<ForgeAPI<F>>,
}
