use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use forge_api::API;
use forge_domain::{Model, Services};
use forge_services::Infrastructure;

use super::app_state::AppState;
use crate::error::Result;

/// Handler for models
pub async fn models<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
) -> Result<Json<Vec<Model>>> {
    let models = state.api.models().await?;
    Ok(Json(models))
}
