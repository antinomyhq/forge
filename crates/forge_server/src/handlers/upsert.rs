use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use forge_api::API;
use forge_domain::{Conversation, Services};
use forge_services::Infrastructure;
use http::StatusCode;

use super::app_state::AppState;
use crate::error::Result;

/// Handler for upserting a conversation
pub async fn upsert_conversation<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
    Json(conversation): Json<Conversation>,
) -> Result<StatusCode> {
    state.api.upsert_conversation(conversation).await?;
    Ok(StatusCode::OK)
}
