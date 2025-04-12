use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use forge_api::API;
use forge_domain::{File, Services};
use forge_services::Infrastructure;

use super::app_state::AppState;
use crate::error::Result;

/// Handler for suggestions
pub async fn suggestions<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
) -> Result<Json<Vec<File>>> {
    let suggestions = state.api.suggestions().await?;
    Ok(Json(suggestions))
}
