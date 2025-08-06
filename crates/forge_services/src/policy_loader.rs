use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::Policies;
use tokio::sync::Mutex;

use crate::{
    DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra,
};

/// A service for loading policy definitions from individual files in the
/// forge/policies directory
pub struct ForgePolicyLoader<F> {
    infra: Arc<F>,

    // Cache is used to maintain the loaded policies
    // for this service instance.
    // So that they could live till user starts a new session.
    cache: Arc<Mutex<Option<Policies>>>,
}

impl<F> ForgePolicyLoader<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, cache: Arc::new(Default::default()) }
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    forge_app::PolicyLoaderService for ForgePolicyLoader<F>
{
    /// Load all policy definitions from the forge/policies directory
    async fn load_policies(&self) -> anyhow::Result<Policies> {
        self.load_policies().await
    }
}

impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    ForgePolicyLoader<F>
{
    /// Load all policy definitions from the forge/policies directory
    async fn load_policies(&self) -> anyhow::Result<Policies> {
        if let Some(policies) = self.cache.lock().await.as_ref() {
            return Ok(policies.clone());
        }
        let policies_dir = self.infra.get_environment().policies_path();
        if !self.infra.exists(&policies_dir).await? {
            return Ok(Policies::new());
        }

        let mut all_policies = Policies::new();

        // Use DirectoryReaderInfra to read all .yml and .yaml files in parallel
        let yaml_files = self
            .infra
            .read_directory_files(&policies_dir, Some("*.yaml"))
            .await
            .with_context(|| "Failed to read policies directory for yaml files")?;
            
        let yml_files = self
            .infra
            .read_directory_files(&policies_dir, Some("*.yml"))
            .await
            .with_context(|| "Failed to read policies directory for yml files")?;

        // Combine both yaml and yml files
        let mut files = yaml_files;
        files.extend(yml_files);

        for (path, content) in files {
            let policy_collection = parse_policy_file(&content)
                .with_context(|| format!("Failed to parse policy {}", path.display()))?;
            
            // Merge the policies from this file into our collection
            for policy in policy_collection.policies {
                all_policies = all_policies.add_policy(policy);
            }
        }

        *self.cache.lock().await = Some(all_policies.clone());

        Ok(all_policies)
    }
}

/// Parse raw content into a Policies collection from YAML
fn parse_policy_file(content: &str) -> Result<Policies> {
    let policies: Policies = serde_yml::from_str(content)
        .with_context(|| "Could not parse policies from YAML")?;

    Ok(policies)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use forge_domain::{Policy, Rule, Permission};

    #[tokio::test]
    async fn test_parse_basic_policies() {
        let content = r#"
policies:
  - permission: allow
    rule:
      read_pattern: "**/*.rs"
  - permission: disallow
    rule:
      read_pattern: "secrets/**/*"
"#;

        let actual = parse_policy_file(content).unwrap();

        assert_eq!(actual.policies.len(), 2);
        
        let first_policy = &actual.policies[0];
        if let Policy::Simple { permission, rule } = first_policy {
            assert_eq!(*permission, Permission::Allow);
            if let Rule::Read(read_rule) = rule {
                assert_eq!(read_rule.read_pattern, "**/*.rs");
            } else {
                panic!("Expected Read rule");
            }
        } else {
            panic!("Expected Simple policy");
        }
    }

    #[tokio::test]
    async fn test_parse_empty_policies() {
        let content = r#"
policies: []
"#;

        let actual = parse_policy_file(content).unwrap();

        assert_eq!(actual.policies.len(), 0);
    }
}