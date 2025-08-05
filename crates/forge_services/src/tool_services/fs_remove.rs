use std::path::Path;
use std::sync::Arc;

use forge_app::{FsRemoveOutput, FsRemoveService, ServiceContext};
use forge_domain::PolicyEngine;

use crate::FileRemoverInfra;
use crate::utils::assert_absolute_path;

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
pub struct ForgeFsRemove<T>(Arc<T>);

impl<T> ForgeFsRemove<T> {
    pub fn new(infra: Arc<T>) -> Self {
        Self(infra)
    }
}

#[async_trait::async_trait]
impl<F: FileRemoverInfra> FsRemoveService for ForgeFsRemove<F> {
    async fn remove(
        &self,
        input_path: String,
        context: &ServiceContext<'_>,
    ) -> anyhow::Result<FsRemoveOutput> {
        let workflow = context.workflow;
        let path = Path::new(&input_path);
        assert_absolute_path(path)?;

        let engine = PolicyEngine::new(workflow);
        let permission_trace = engine.can_write(path);

        // Check permission and handle according to policy
        match permission_trace.value {
            forge_domain::Permission::Disallow => {
                return Err(anyhow::anyhow!(
                    "Operation denied by policy at {}:{}. Read access to '{}' is not permitted.",
                    permission_trace
                        .file
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    permission_trace.line.unwrap_or(0),
                    path.display()
                ));
            }
            forge_domain::Permission::Allow => {
                // Continue with the operation
            }
            forge_domain::Permission::Confirm => {
                // Request user confirmation
                match context.request_confirmation() {
                    forge_domain::UserResponse::Accept
                    | forge_domain::UserResponse::AcceptAndRemember => {
                        // User accepted the operation, continue
                    }
                    forge_domain::UserResponse::Reject => {
                        return Err(anyhow::anyhow!(
                            "Operation rejected by user for file: {}",
                            path.display()
                        ));
                    }
                }
            }
        }

        self.0.remove(path).await?;

        Ok(FsRemoveOutput {})
    }
}
