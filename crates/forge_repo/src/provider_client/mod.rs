pub(crate) mod anthropic;
pub(crate) mod bedrock;
mod event;
#[cfg(test)]
mod mock_server;
pub(crate) mod openai;
pub(crate) mod retry;
mod utils;

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
