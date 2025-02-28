use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::{middleware, Router};

use super::AuthUser;
use crate::presentation::handlers::{
    chat_completion, create_api_key, delete_api_key, get_by_key_id, get_model_parameters,
    list_api_keys, list_models,
};
use crate::presentation::middleware::auth::{api_key_auth, jwt_auth};
use crate::service::api_keys::ApiKeyService;
use crate::service::authorization::AuthorizeService;
use crate::service::proxy::ProxyService;

pub fn api_key_routes<T, Out>(service: Arc<ApiKeyService>, auth_service: Arc<T>) -> Router
where
    T: AuthorizeService<Output = Out> + 'static,
    Out: Into<AuthUser> + 'static,
{
    Router::new()
        .route("/api/v1/user/keys", post(create_api_key))
        .route("/api/v1/user/keys", get(list_api_keys))
        .route("/api/v1/user/keys/{id}", get(get_by_key_id))
        .route("/api/v1/user/keys/{id}", delete(delete_api_key))
        .layer(middleware::from_fn_with_state(auth_service, jwt_auth))
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
            api_key_auth,
        ))
        .with_state(proxy_service)
}

pub fn app<T, Out>(
    api_key_service: Arc<ApiKeyService>,
    proxy_service: Arc<ProxyService>,
    auth_service: Arc<T>,
) -> Router
where
    T: AuthorizeService<Output = Out> + 'static,
    Out: Into<AuthUser> + 'static,
{
    Router::new()
        .merge(api_key_routes(api_key_service.clone(), auth_service))
        .merge(proxy_routes(proxy_service, api_key_service))
}
