use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use forge_domain::Services;
use forge_services::Infrastructure;

use crate::handlers::*;

/// Create the application router
pub fn create_router<F: Services + Infrastructure + Send + Sync + 'static>(
    app_state: Arc<AppState<F>>,
) -> Router {
    Router::new()
        // API endpoints
        .route("/api/suggestions", get(suggestions::<F>))
        .route("/api/tools", get(tools::<F>))
        .route("/api/models", get(models::<F>))
        .route("/api/chat", post(chat::<F>))
        .route("/api/environment", get(environment::<F>))
        .route("/api/init", post(init::<F>))
        .route("/api/load", get(load::<F>))
        .route("/api/conversation/:conversation_id", get(conversation::<F>))
        .route("/api/conversation", post(upsert_conversation::<F>))
        .route(
            "/api/conversation/:conversation_id/variable",
            get(get_variable::<F>).put(set_variable::<F>),
        )
        .with_state(app_state)
}
