use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use forge_api::API;
use forge_domain::{Conversation, ConversationId, Services};
use forge_services::Infrastructure;

use super::app_state::AppState;
use crate::error::{Error, Result};

/// Handler for getting a conversation
pub async fn conversation<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
    AxumPath(conversation_id): AxumPath<ConversationId>,
) -> Result<Json<Conversation>> {
    let conversation = state
        .api
        .conversation(&conversation_id)
        .await?
        .ok_or_else(|| Error::ConversationNotFound(conversation_id.to_string()))?;

    Ok(Json(conversation))
}
