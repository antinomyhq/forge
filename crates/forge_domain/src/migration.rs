use std::path::PathBuf;

use crate::ProviderId;

/// Result of credential migration from environment variables to file.
/// Only returned when credentials were actually migrated (Some).
/// None indicates file already exists or no credentials to migrate.
#[derive(Debug, Clone)]
pub struct MigrationResult {
    /// Path to the credentials file
    pub credentials_path: PathBuf,
    /// Providers that were migrated
    pub migrated_providers: Vec<ProviderId>,
}

impl MigrationResult {
    /// Creates a result indicating successful migration
    pub fn new(credentials_path: PathBuf, migrated_providers: Vec<ProviderId>) -> Self {
        Self { credentials_path, migrated_providers }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_migration_result() {
        let path = PathBuf::from("/test/.credentials.json");
        let providers = vec![ProviderId::OPENAI, ProviderId::ANTHROPIC];

        let actual = MigrationResult::new(path.clone(), providers.clone());

        assert_eq!(actual.credentials_path, path);
        assert_eq!(actual.migrated_providers, providers);
    }
}
