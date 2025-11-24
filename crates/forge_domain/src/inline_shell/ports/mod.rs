/// Ports module for inline shell operations
///
/// This module contains the interface definitions (ports) that define
/// the contracts between the domain layer and infrastructure layer
/// for inline shell command operations.
pub mod command_executor;
pub mod policy_repository;
pub mod security_analysis;

pub use command_executor::CommandExecutor;
pub use policy_repository::PolicyRepository;
pub use security_analysis::SecurityAnalysis;
