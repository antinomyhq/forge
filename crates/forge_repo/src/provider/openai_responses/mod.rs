/// OpenAI Responses (Codex) provider modules.
/// - `request.rs`: builds async-openai CreateResponse from domain context,
///   including tool schema normalization.
/// - `response.rs`: parses Responses API outputs and streaming events into
///   ChatCompletionMessage.
/// - `repository.rs`: provider client (headers/endpoints) and ChatRepository
///   implementation with retry handling.
mod repository;
mod request;
mod response;

pub use repository::OpenAIResponsesResponseRepository;
