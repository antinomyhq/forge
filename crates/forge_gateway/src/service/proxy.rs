use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use forge_domain::{Model, ModelId, Parameters, ProviderService};
use futures::TryStreamExt;
use futures_util::StreamExt;

use crate::error::{Error, Result};
use crate::presentation::dto::ChatCompletionRequest;

pub struct ProxyService {
    provider: Box<dyn ProviderService>,
}

impl ProxyService {
    pub fn new(provider: Box<dyn ProviderService>) -> Self {
        Self { provider }
    }

    pub async fn chat_completion(&self, req: ChatCompletionRequest) -> Result<impl IntoResponse> {
        let model_id = ModelId::new(req.model.clone());
        let context = req.into();

        let stream = self
            .provider
            .chat(&model_id, context)
            .await
            .map_err(|e| Error::Service(format!("Chat completion failed: {}", e)))?;

        // Convert the chat completion stream to SSE events
        let event_stream = stream
            .map_ok(|msg| {
                Event::default()
                    .data(serde_json::to_string(&msg).expect("Failed to serialize message"))
            })
            .map(|r| {
                Ok::<Event, std::convert::Infallible>(r.expect("Failed to process stream event"))
            });

        Ok(Sse::new(event_stream).keep_alive(KeepAlive::default()))
    }

    pub async fn list_models(&self) -> Result<Vec<Model>> {
        self.provider
            .models()
            .await
            .map_err(|e| Error::Service(e.to_string()))
    }

    pub async fn get_model_parameters(&self, model_id: String) -> Result<Parameters> {
        let model_id = ModelId::new(model_id);
        self.provider
            .parameters(&model_id)
            .await
            .map_err(|e| Error::Service(format!("Failed to get model parameters: {}", e)))
    }
}
