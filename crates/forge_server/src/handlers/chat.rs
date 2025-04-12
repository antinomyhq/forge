use std::sync::Arc;

use axum::extract::State;
use axum::response::sse::Event;
use axum::response::Sse;
use axum::Json;
use forge_api::API;
use forge_domain::{AgentMessage, ChatRequest, ChatResponse, Services};
use forge_services::Infrastructure;
use futures::stream::{Stream, StreamExt};

use super::app_state::AppState;
use crate::error::Result;

/// Helper function to convert a single agent message to an SSE Event
///
/// Serializes the message to JSON and wraps it in an SSE Event ready for
/// delivery
fn into_event(message: anyhow::Result<AgentMessage<ChatResponse>>) -> Result<Event> {
    let message = serde_json::to_string(&message?)?;
    Ok(Event::default().data(message))
}

/// Handler for chat
pub async fn chat<F: Services + Infrastructure>(
    State(state): State<Arc<AppState<F>>>,
    Json(chat_request): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event>>>> {
    let stream = state.api.chat(chat_request).await?;
    Ok(Sse::new(stream.map(into_event)))
}
