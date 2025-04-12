use std::sync::Arc;

use axum::http::Method;
use forge_api::ForgeAPI;
use forge_domain::Services;
use forge_infra::ForgeInfra;
use forge_services::{ForgeServices, Infrastructure};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::{get_socket_addr, ServerConfig};
use crate::error::Result;
use crate::handlers::AppState;
use crate::routes::create_router;

/// Forge HTTP Server
pub struct ForgeServer<F = ForgeServices<ForgeInfra>> {
    api: Arc<ForgeAPI<F>>,
    config: ServerConfig,
}

impl ForgeServer<ForgeServices<ForgeInfra>> {
    /// Create a new server with default configuration
    pub fn new(config: ServerConfig) -> Self {
        let api = Arc::new(ForgeAPI::init(config.restricted));
        Self { api, config }
    }

    /// Create a new server with custom API instance
    pub fn with_api(api: ForgeAPI<ForgeServices<ForgeInfra>>, config: ServerConfig) -> Self {
        Self { api: Arc::new(api), config }
    }
}

impl<F> ForgeServer<F>
where
    F: Services + Infrastructure + Send + Sync + 'static,
{
    /// Create a new server with custom services and infrastructure
    pub fn with_custom_api(api: ForgeAPI<F>, config: ServerConfig) -> Self {
        Self { api: Arc::new(api), config }
    }

    /// Start the server
    pub async fn start(self) -> Result<()> {
        let app_state = Arc::new(AppState { api: self.api });
        let mut app = create_router(app_state);

        // Apply CORS if enabled
        if self.config.enable_cors {
            app = app.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods([
                        Method::GET,
                        Method::POST,
                        Method::PUT,
                        Method::DELETE,
                        Method::OPTIONS,
                    ])
                    .allow_headers(Any),
            );
        }

        // Add tracing
        app = app.layer(TraceLayer::new_for_http());

        let addr = get_socket_addr(&self.config);
        info!("Starting Forge server on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}
