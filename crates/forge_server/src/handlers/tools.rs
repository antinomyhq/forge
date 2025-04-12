use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use forge_api::API;
use forge_domain::{Services, ToolDefinition};
use forge_services::Infrastructure;

use super::app_state::AppState;

/// Handler for tools
pub async fn tools<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
) -> Json<Vec<ToolDefinition>> {
    let tools = state.api.tools().await;
    Json(tools)
}
