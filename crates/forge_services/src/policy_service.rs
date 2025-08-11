use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use forge_app::PolicyLoaderService;
use forge_app::domain::{Policy, PolicyConfig};
use forge_display::DiffFormat;

use crate::{
    DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra,
};

/// A service for loading policy definitions from individual files in the
/// forge/policies directory
pub struct ForgePolicyLoader<F> {
    infra: Arc<F>,
}

impl<F> ForgePolicyLoader<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    PolicyLoaderService for ForgePolicyLoader<F>
{
    /// Load all policy definitions from the forge/policies directory
    async fn read_policies(&self) -> anyhow::Result<Option<PolicyConfig>> {
        self.read_policies().await
    }

    async fn modify_policy(&self, policy: Policy) -> Result<String> {
        self.modify_policy(policy).await
    }

    fn policies_path(&self) -> PathBuf {
        self.policies_path()
    }

    async fn init_policies(&self) -> Result<()> {
        self.init_policies().await
    }
}

impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra> ForgePolicyLoader<F> {
    fn policies_path(&self) -> PathBuf {
        self.infra.get_environment().policies_path()
    }
    /// Load all policy definitions from the forge/policies directory
    async fn read_policies(&self) -> anyhow::Result<Option<PolicyConfig>> {
        // NOTE: we must not cache policies, as they can change at runtime.

        let policies_path = self.policies_path();
        if !self.infra.exists(&policies_path).await? {
            // If the policies file does not exist, return None
            return Ok(None);
        }

        let content = self.infra.read_utf8(&policies_path).await?;

        let policies = parse_policy_file(&content)
            .with_context(|| format!("Failed to parse policy {}", policies_path.display()))?;

        Ok(Some(policies))
    }
    /// Add or modify a policy in the policies file and return a diff of the
    /// changes
    async fn modify_policy(&self, policy: Policy) -> anyhow::Result<String> {
        let policies_path = self.policies_path();

        // Read current content (if file exists)
        let old_content = if self.infra.exists(&policies_path).await? {
            self.infra.read_utf8(&policies_path).await?
        } else {
            String::new()
        };

        // Load current policies or create empty collection
        let mut policies = if old_content.is_empty() {
            // If the file is empty or does not exist, start with default policies
            PolicyConfig::with_defaults()
        } else {
            parse_policy_file(&old_content).with_context(|| {
                format!(
                    "Failed to parse existing policies {}",
                    policies_path.display()
                )
            })?
        };

        // Add the new policy to the collection
        policies = policies.add_policy(policy);

        // Serialize the updated policies to YAML
        let new_content = serde_yml::to_string(&policies)
            .with_context(|| "Failed to serialize policies to YAML")?;

        // Write the updated content
        self.infra
            .write(&policies_path, Bytes::from(new_content.to_owned()), true)
            .await?;

        // Generate and return the diff
        let diff_result = DiffFormat::format(&old_content, &new_content);
        Ok(diff_result.diff().to_string())
    }

    async fn init_policies(&self) -> Result<()> {
        let policies_path = self.policies_path();

        // Check if the file already exists
        if self.infra.exists(&policies_path).await? {
            // If it exists, do nothing
            return Ok(());
        }

        // Get the default policies content
        let default_policies = PolicyConfig::with_defaults();
        let content = serde_yml::to_string(&default_policies)
            .with_context(|| "Failed to serialize default policies to YAML")?;

        // Write the default policies to the file
        self.infra
            .write(&policies_path, Bytes::from(content), false)
            .await?;

        Ok(())
    }
}

/// Parse raw content into a Policies collection from YAML
fn parse_policy_file(content: &str) -> Result<PolicyConfig> {
    let policies: PolicyConfig =
        serde_yml::from_str(content).with_context(|| "Could not parse policies from YAML")?;

    Ok(policies)
}
