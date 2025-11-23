/// Result of a policy check operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    /// Whether operation is allowed
    pub allowed: bool,
    /// Path to policy file that was created/modified (if any)
    pub path: Option<std::path::PathBuf>,
}

/// Port for policy repository operations
///
/// This trait defines the interface for accessing and managing policies,
/// providing an abstraction over policy storage and retrieval operations.
#[allow(async_fn_in_trait)]
pub trait PolicyRepository: Send + Sync {
    /// Check if an operation is allowed based on policies
    async fn check_operation_permission(
        &self,
        operation: &crate::PermissionOperation,
    ) -> anyhow::Result<PolicyDecision>;

    /// Load all policy definitions
    async fn load_policies(&self) -> anyhow::Result<Option<crate::PolicyConfig>>;

    /// Add or modify a policy
    async fn save_policy(&self, policy: crate::Policy) -> anyhow::Result<()>;
}
