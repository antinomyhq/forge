mod anthropic;
mod bedrock;
mod bedrock_cache;
mod chat;
mod event;
#[cfg(test)]
mod mock_server;
mod openai;
mod provider_repo;
mod retry;
mod utils;

pub use chat::*;
pub use provider_repo::*;

/// Trait for converting types into domain types
trait IntoDomain {
    type Domain;
    fn into_domain(self) -> Self::Domain;
}

/// Trait for converting from domain types
trait FromDomain<T> {
    fn from_domain(value: T) -> anyhow::Result<Self>
    where
        Self: Sized;
}
