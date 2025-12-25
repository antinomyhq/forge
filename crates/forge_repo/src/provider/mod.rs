mod anthropic;
mod bedrock;
mod bedrock_cache;
mod event;
#[cfg(test)]
mod mock_server;
mod openai;
mod retry;
mod router;
mod utils;

pub use router::*;

/// Trait for converting types into domain types
pub(crate) trait IntoDomain {
    type Domain;
    fn into_domain(self) -> Self::Domain;
}

/// Trait for converting from domain types
pub(crate) trait FromDomain<T> {
    fn from_domain(value: T) -> anyhow::Result<Self>
    where
        Self: Sized;
}
