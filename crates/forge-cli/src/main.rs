use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::State;

mod app;
mod completion;
use axum::response::sse::{Event, Sse};
use axum::routing::get;
use axum::Router;
use clap::Parser;
use completion::{get_completions, Completion};
use forge_cli::cli::Cli;
use forge_cli::Result;
use futures::stream::Stream;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging with level from CLI
    tracing_subscriber::fmt()
        .with_max_level(cli.log_level.clone().unwrap_or_default())
        .init();

    // Create broadcast channel for SSE

    let state = Arc::new(app::AppState::<String>::default());

    // Setup HTTP server
    let app = Router::new()
        .route("/conversation", get(conversation_handler))
        .route("/completions", get(completions_handler))
        .layer(CorsLayer::new().allow_origin(Any))
        .with_state(state.clone());

    // Spawn HTTP server
    let server = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
            .await
            .unwrap();
        info!("Server running on http://127.0.0.1:3000");
        axum::serve(listener, app).await.unwrap();
    });

    // Wait for server to complete (though it runs indefinitely)
    let _ = server.await;

    Ok(())
}

async fn completions_handler() -> axum::Json<Vec<Completion>> {
    axum::Json(get_completions().await)
}

async fn conversation_handler(
    State(state): State<Arc<app::AppState<String>>>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    Sse::new(state.as_stream().await)
}
