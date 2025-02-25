use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::{middleware, Router};
use clerk_rs::validators::authorizer::ClerkAuthorizer;

use crate::presentation::handlers::{
    chat_completion, create_api_key, delete_api_key, get_by_key_id, get_model_parameters,
    list_api_keys, list_models,
};
use crate::presentation::middleware::auth::{clerk_auth, validate_api_key};
use crate::service::api_keys::ApiKeyService;
use crate::service::proxy::ProxyService;

pub fn api_key_routes(service: Arc<ApiKeyService>, clerk: Arc<ClerkAuthorizer>) -> Router {
    Router::new()
        .route("/api/v1/user/keys", post(create_api_key))
        .route("/api/v1/user/keys", get(list_api_keys))
        .route("/api/v1/user/keys/{id}", get(get_by_key_id))
        .route("/api/v1/user/keys/{id}", delete(delete_api_key))
        .layer(middleware::from_fn_with_state(clerk, clerk_auth))
        .with_state(service)
}

pub fn proxy_routes(
    proxy_service: Arc<ProxyService>,
    api_key_service: Arc<ApiKeyService>,
) -> Router {
    Router::new()
        .route("/api/v1/chat/completions", post(chat_completion))
        .route("/api/v1/models", get(list_models))
        .route("/api/v1/parameters/{*id}", get(get_model_parameters))
        .without_v07_checks()
        .layer(middleware::from_fn_with_state(
            api_key_service.clone(),
            validate_api_key,
        ))
        .with_state(proxy_service)
}

pub fn app(
    api_key_service: Arc<ApiKeyService>,
    proxy_service: Arc<ProxyService>,
    clerk: Arc<ClerkAuthorizer>,
) -> Router {
    Router::new()
        .merge(api_key_routes(api_key_service.clone(), clerk))
        .merge(proxy_routes(proxy_service, api_key_service))
}
