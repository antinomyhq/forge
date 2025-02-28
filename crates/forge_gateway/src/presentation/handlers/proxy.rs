use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use forge_domain::{Model, Parameters};
use validator::Validate;

use crate::error::Result;
use crate::presentation::dto::ChatCompletionRequest;
use crate::service::proxy::ProxyService;
use crate::Error;

#[axum::debug_handler]
pub async fn chat_completion(
    State(service): State<Arc<ProxyService>>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<impl IntoResponse> {
    if let Err(e) = request.validate() {
        return Err(Error::BadRequest(e));
    }
    service.chat_completion(request).await
}

#[axum::debug_handler]
pub async fn list_models(State(service): State<Arc<ProxyService>>) -> Result<Json<Vec<Model>>> {
    let models = service.list_models().await?;
    Ok(Json(models))
}

#[axum::debug_handler]
pub async fn get_model_parameters(
    State(service): State<Arc<ProxyService>>,
    Path(model_id): Path<String>,
) -> Result<Json<Parameters>> {
    let parameters = service.get_model_parameters(model_id).await?;
    Ok(Json(parameters))
}
