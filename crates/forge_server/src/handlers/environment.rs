use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use forge_api::API;
use forge_domain::{Environment, Services};
use forge_services::Infrastructure;

use super::app_state::AppState;

/// Handler for environment
pub async fn environment<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
) -> Json<Environment> {
    let env = state.api.environment();
    Json(env)
}
