use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use forge_api::API;
use forge_domain::{ConversationId, Services};
use forge_services::Infrastructure;
use http::StatusCode;
use serde::Deserialize;
use serde_json::Value;

use super::app_state::AppState;
use crate::error::Result;

/// Parameters for variable operations
#[derive(Deserialize)]
pub struct VariableParams {
    pub key: String,
}

/// Handler for getting a variable
pub async fn get_variable<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
    AxumPath(conversation_id): AxumPath<ConversationId>,
    Query(params): Query<VariableParams>,
) -> Result<Json<Option<Value>>> {
    let value = state
        .api
        .get_variable(&conversation_id, &params.key)
        .await?;
    Ok(Json(value))
}

/// Handler for setting a variable
pub async fn set_variable<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
    AxumPath(conversation_id): AxumPath<ConversationId>,
    Query(params): Query<VariableParams>,
    Json(value): Json<Value>,
) -> Result<StatusCode> {
    state
        .api
        .set_variable(&conversation_id, params.key, value)
        .await?;

    Ok(StatusCode::OK)
}
