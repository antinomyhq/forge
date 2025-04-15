//! Serialization helpers for JSON-RPC

use serde_json::{json, Value};
use tracing::info;

/// Convert a chat response to JSON for streaming
pub fn chat_response_to_json<T: serde::Serialize>(response: T) -> Value {
    // Log that we're serializing something
    info!("Attempting to serialize response to JSON");

    // First attempt direct serialization
    match serde_json::to_value(&response) {
        Ok(value) => {
            // Log success and the resulting value
            if let Ok(value_str) = serde_json::to_string(&value) {
                info!("Successfully serialized response: {}", value_str);
            } else {
                info!("Successfully serialized response but couldn't stringify it");
            }

            // If we have a serializable object, add a bit more context for debugging
            // but typically we'll use the direct format in stream_handler
            value
        }
        Err(e) => {
            // Log the error
            info!("Failed to serialize response: {}", e);

            // Report serialization error
            json!({
                "error": format!("Failed to serialize response: {}", e)
            })
        }
    }
}
