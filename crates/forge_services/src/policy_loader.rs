use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::Policies;

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
}

impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    ForgePolicyLoader<F>
{
    /// Load all policy definitions from the forge/policies directory
    async fn load_policies(&self) -> anyhow::Result<Policies> {
        // NOTE: we must not cache policies, as they can change at runtime.

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

        Ok(all_policies)
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
                    permission == &Permission::Disallow && matches!(rule, Rule::Execute(_))
                } else {
                    false
                }
            })
            .expect("Should find execute disallow policy");

        if let Policy::Simple { permission, rule } = execute_policy_disallow {
            assert_eq!(permission, &Permission::Disallow);
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
}
