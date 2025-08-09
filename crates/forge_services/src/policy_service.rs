use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use forge_display::DiffFormat;
use forge_domain::{Policies, Policy};

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
    forge_app::PolicyLoaderService for ForgePolicyLoader<F>
{
    /// Load all policy definitions from the forge/policies directory
    async fn load_policies(&self) -> anyhow::Result<Policies> {
        self.load_policies().await
    }

    async fn modify_policy(&self, policy: Policy) -> Result<String> {
        self.modify_policy(policy).await
    }
}

impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra> ForgePolicyLoader<F> {
    /// Load all policy definitions from the forge/policies directory
    async fn load_policies(&self) -> anyhow::Result<Policies> {
        // NOTE: we must not cache policies, as they can change at runtime.

        let policies_path = self.infra.get_environment().policies_path();
        if !self.infra.exists(&policies_path).await? {
            // if the policies file does not exist, create it with default policies.
            let default_policies = Policies::with_defaults();
            let content = serde_yml::to_string(&default_policies)
                .with_context(|| "Failed to serialize default policies to YAML")?;
            self.infra
                .write(&policies_path, Bytes::from(content), false)
                .await?;
            return Ok(default_policies);
        }

        let content = self.infra.read_utf8(&policies_path).await?;

        parse_policy_file(&content)
            .with_context(|| format!("Failed to parse policy {}", policies_path.display()))
    }
    /// Add or modify a policy in the policies file and return a diff of the
    /// changes
    async fn modify_policy(&self, policy: Policy) -> anyhow::Result<String> {
        let policies_path = self.infra.get_environment().policies_path();

        // Read current content (if file exists)
        let old_content = if self.infra.exists(&policies_path).await? {
            self.infra.read_utf8(&policies_path).await?
        } else {
            String::new()
        };

        // Load current policies or create empty collection
        let mut policies = if old_content.is_empty() {
            // If the file is empty or does not exist, start with default policies
            Policies::with_defaults()
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
}

/// Parse raw content into a Policies collection from YAML
fn parse_policy_file(content: &str) -> Result<Policies> {
    let policies: Policies =
        serde_yml::from_str(content).with_context(|| "Could not parse policies from YAML")?;

    Ok(policies)
}

#[cfg(test)]
mod tests {
    use forge_domain::{Permission, Policy, Rule};
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_parse_basic_policies() {
        let content = include_str!("fixtures/policies/basic.yml");

        let actual = parse_policy_file(content).unwrap();

        assert_eq!(actual.policies.len(), 2);

        let first_policy = actual.policies.iter().next().unwrap();
        if let Policy::Simple { permission, rule } = first_policy {
            assert_eq!(permission, &Permission::Allow);
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
        let content = include_str!("fixtures/policies/empty.yml");

        let actual = parse_policy_file(content).unwrap();

        assert_eq!(actual.policies.len(), 0);
    }

    #[tokio::test]
    async fn test_parse_comprehensive_policies() {
        let content = include_str!("fixtures/policies/comprehensive.yml");

        let actual = parse_policy_file(content).unwrap();

        assert_eq!(actual.policies.len(), 4);

        let policies_vec: Vec<_> = actual.policies.iter().collect();

        // Find the read policy (Allow permission with read rule)
        let read_policy = policies_vec
            .iter()
            .find(|policy| {
                if let Policy::Simple { permission, rule } = policy {
                    permission == &Permission::Allow && matches!(rule, Rule::Read(_))
                } else {
                    false
                }
            })
            .expect("Should find read policy");

        if let Policy::Simple { permission, rule } = read_policy {
            assert_eq!(permission, &Permission::Allow);
            if let Rule::Read(read_rule) = rule {
                assert_eq!(read_rule.read_pattern, "**/*.{rs,js,ts,py}");
            } else {
                panic!("Expected Read rule");
            }
        } else {
            panic!("Expected Simple policy");
        }

        // Find the write policy (Confirm permission with write rule)
        let write_policy = policies_vec
            .iter()
            .find(|policy| {
                if let Policy::Simple { permission, rule } = policy {
                    permission == &Permission::Confirm && matches!(rule, Rule::Write(_))
                } else {
                    false
                }
            })
            .expect("Should find write policy");

        if let Policy::Simple { permission, rule } = write_policy {
            assert_eq!(permission, &Permission::Confirm);
            if let Rule::Write(write_rule) = rule {
                assert_eq!(write_rule.write_pattern, "src/**/*");
            } else {
                panic!("Expected Write rule");
            }
        } else {
            panic!("Expected Simple policy");
        }

        // Find the execute disallow policy
        let execute_policy_disallow = policies_vec
            .iter()
            .find(|policy| {
                if let Policy::Simple { permission, rule } = policy {
                    permission == &Permission::Deny && matches!(rule, Rule::Execute(_))
                } else {
                    false
                }
            })
            .expect("Should find execute disallow policy");

        if let Policy::Simple { permission, rule } = execute_policy_disallow {
            assert_eq!(permission, &Permission::Deny);
            if let Rule::Execute(execute_rule) = rule {
                assert_eq!(execute_rule.command_pattern, "rm -rf /*");
            } else {
                panic!("Expected Execute rule");
            }
        } else {
            panic!("Expected Simple policy");
        }

        // Find the execute allow policy (Allow permission with "cargo*" pattern)
        let execute_policy_allow = policies_vec
            .iter()
            .find(|policy| {
                if let Policy::Simple { permission, rule } = policy {
                    permission == &Permission::Allow
                        && if let Rule::Execute(exec_rule) = rule {
                            exec_rule.command_pattern == "cargo*"
                        } else {
                            false
                        }
                } else {
                    false
                }
            })
            .expect("Should find execute allow policy");

        if let Policy::Simple { permission, rule } = execute_policy_allow {
            assert_eq!(permission, &Permission::Allow);
            if let Rule::Execute(execute_rule) = rule {
                assert_eq!(execute_rule.command_pattern, "cargo*");
            } else {
                panic!("Expected Execute rule");
            }
        } else {
            panic!("Expected Simple policy");
        }
    }

    #[tokio::test]
    async fn test_parse_invalid_yaml() {
        let content = include_str!("fixtures/policies/invalid.yml");

        let result = parse_policy_file(content);
        assert!(result.is_err());
    }
    #[tokio::test]
    async fn test_modify_policy_logic() {
        use forge_domain::{Permission, Rule, WriteRule};

        // Test the core logic by parsing existing policies and adding a new one
        let existing_content = "policies: []";
        let mut policies = parse_policy_file(existing_content).unwrap();

        let new_policy = Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Write(WriteRule { write_pattern: "src/**/*.rs".to_string(), working_directory: None }),
        };

        policies = policies.add_policy(new_policy.clone());

        // Serialize back to YAML
        let new_content = serde_yml::to_string(&policies).unwrap();

        // Generate diff
        let diff_result = DiffFormat::format(existing_content, &new_content);
        let actual = diff_result.diff();

        // Should contain the new policy
        assert!(actual.contains("permission: Allow"));
        assert!(actual.contains("src/**/*.rs"));
    }
}
