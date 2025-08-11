// Due to a conflict between names of Anthropic and OpenAI we will namespace the
// DTOs instead of using Prefixes for type names
pub mod anthropic;
mod app_config;
pub mod openai;

pub use app_config::*;
