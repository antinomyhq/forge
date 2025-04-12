use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use forge_api::API;
use forge_domain::{ConversationId, Services, Workflow};
use forge_services::Infrastructure;

use super::app_state::AppState;
use crate::error::Result;

/// Handler for initializing a conversation
pub async fn init<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
    workflow: Option<Json<Workflow>>,
) -> Result<Json<ConversationId>> {
    let conversation_id = match workflow {
        Some(Json(workflow)) => state.api.init(workflow).await?,
        None => state.api.init(Workflow::default()).await?,
    };
    Ok(Json(conversation_id))
}
