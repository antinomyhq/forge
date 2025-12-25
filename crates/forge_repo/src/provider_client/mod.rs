mod anthropic;
mod bedrock;
mod bedrock_cache;
mod event;
#[cfg(test)]
mod mock_server;
mod openai;
mod retry;
mod utils;

pub use anthropic::*;
pub use bedrock::*;
pub use openai::*;
pub use retry::*;

/// Trait for converting types into domain types
pub trait IntoDomain {
    type Domain;
    fn into_domain(self) -> Self::Domain;
}

/// Trait for converting from domain types
pub trait FromDomain<T> {
    fn from_domain(value: T) -> anyhow::Result<Self>
    where
        Self: Sized;
}
