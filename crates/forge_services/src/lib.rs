mod agent_registry;
mod attachment;
mod auth;
mod clipper;
mod command_loader;
mod conversation;
mod custom_instructions;
mod discovery;
mod env;
mod error;
mod forge_services;
mod http;
mod mcp;
mod policy;
mod preferences;
mod provider;
mod provider_auth;
mod range;
mod template;
mod tool_services;
mod utils;
mod workflow;

pub use agent_registry::*;
pub use clipper::*;
pub use command_loader::*;
pub use custom_instructions::*;
pub use discovery::*;
pub use error::*;
pub use forge_services::*;
pub use policy::*;
pub use preferences::*;
pub use provider_auth::*;

/// Converts a type from its external representation into its domain model
/// representation.
pub trait IntoDomain {
    type Domain;

    fn into_domain(self) -> Self::Domain;
}
