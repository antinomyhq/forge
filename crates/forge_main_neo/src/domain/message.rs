use forge_api::ChatResponse;

/// Enum to differentiate between user and assistant messages
#[derive(Debug, Clone)]
pub enum Message {
    User(String),
    Assistant(ChatResponse),
}
