mod agent_registry;
mod app_config;
mod attachment;
mod auth;
mod clipper;
mod command;
mod config_watcher;
mod context_engine;
mod conversation;
mod discovery;
mod elicitation_dispatcher;
mod error;
mod fd;
mod fd_git;
mod fd_walker;
mod file_changed_watcher;
mod forge_services;
mod fs_watcher_core;
mod hook_runtime;
// Re-export shell executor for integration/performance tests.
// Re-export workspace trust helper for the CLI `forge trust` command.
pub use hook_runtime::config_loader::accept_workspace_trust;
pub use hook_runtime::shell::{ForgeShellHookExecutor, PromptHandler};
mod instructions;
mod mcp;
mod policy;

mod provider_auth;
mod provider_service;
mod range;
mod sync;
mod template;
mod tool_services;
mod utils;
pub mod worktree_manager;

pub use app_config::*;
pub use clipper::*;
pub use command::*;
pub use config_watcher::*;
pub use context_engine::*;
pub use discovery::*;
pub use elicitation_dispatcher::ForgeElicitationDispatcher;
pub use error::*;
pub use file_changed_watcher::*;
pub use forge_services::*;
pub use instructions::*;
pub use policy::*;
pub use provider_auth::*;

/// Converts a type from its external representation into its domain model
/// representation.
pub trait IntoDomain {
    type Domain;

    fn into_domain(self) -> Self::Domain;
}

/// Converts a domain model type into its external representation.
pub trait FromDomain<T> {
    /// Converts from a domain type to the external type
    ///
    /// # Errors
    ///
    /// Returns an error if the conversion fails
    fn from_domain(value: T) -> anyhow::Result<Self>
    where
        Self: Sized;
}
