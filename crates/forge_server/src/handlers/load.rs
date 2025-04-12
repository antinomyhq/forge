use std::path::Path;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use forge_api::API;
use forge_domain::{Services, Workflow};
use forge_services::Infrastructure;
use serde::Deserialize;

use super::app_state::AppState;
use crate::error::Result;

/// Parameters for load handler
#[derive(Deserialize)]
pub struct LoadParams {
    pub path: Option<String>,
}

/// Handler for loading a workflow
pub async fn load<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
    Query(params): Query<LoadParams>,
) -> Result<Json<Workflow>> {
    let path_ref = params.path.as_ref().map(Path::new);
    let workflow = state.api.load(path_ref).await?;
    Ok(Json(workflow))
}
