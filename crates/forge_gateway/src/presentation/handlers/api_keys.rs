use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use crate::error::{Error, Result};
use crate::presentation::dto::{ApiKeyResponse, CreateApiKeyRequest};
use crate::presentation::middleware::auth::AuthUser;
use crate::service::api_keys::ApiKeyService;

#[axum::debug_handler]
pub async fn create_api_key(
    State(service): State<Arc<ApiKeyService>>,
    auth_user: AuthUser,
    Json(request): Json<CreateApiKeyRequest>,
) -> Result<impl IntoResponse> {
    if let Err(e) = request.validate() {
        return Err(Error::BadRequest(e));
    }
    let api_key = service.create_api_key(&auth_user.id, &request.name).await?;
    let api_key_response: ApiKeyResponse = api_key.into();
    Ok((StatusCode::CREATED, Json(api_key_response)))
}

#[axum::debug_handler]
pub async fn list_api_keys(
    State(service): State<Arc<ApiKeyService>>,
    auth_user: AuthUser,
) -> Result<Json<Vec<ApiKeyResponse>>> {
    let api_keys = service.list_api_keys(&auth_user.id).await?;
    Ok(Json(api_keys.into_iter().map(Into::into).collect()))
}

#[axum::debug_handler]
pub async fn delete_api_key(
    State(service): State<Arc<ApiKeyService>>,
    auth_user: AuthUser,
    Path(key_id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    let _ = service.delete_api_key(&auth_user.id, key_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_by_key_id(
    State(service): State<Arc<ApiKeyService>>,
    auth_user: AuthUser,
    Path(key_id): Path<Uuid>,
) -> Result<Json<ApiKeyResponse>> {
    let api_key = service.find_by_key_id(&auth_user.id, key_id).await?;
    Ok(Json(api_key.into()))
}
