use std::path::PathBuf;

use anyhow::Context;
use forge_app::{CodebaseIndexOutput, IndexCodebaseService, Shard};
use forge_domain::{IndexingConfig, ShardingConfig, ShardingType};
use yek::count_tokens;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub sharding: Option<Sharding>,
    pub ignore_patterns: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum Sharding {
    Token(usize),
    Byte(usize),
}

impl Sharding {
    /// Shards the contents depending on count function and by limit.
    fn shard_by_fn(limit: usize, contents: Vec<String>, count: fn(&str) -> usize) -> Vec<Shard> {
        let mut shards = Vec::new();
        let mut current_value = 0;
        let mut current_content = String::new();

        for file in contents {
            let file_value = count(&file);
            if current_value + file_value > limit && !current_content.is_empty() {
                shards.push(Shard(std::mem::take(&mut current_content)));
                current_value = 0;
            }
            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(&file);
            current_value += file_value;
        }
        // Add the final shard if there's any content
        if !current_content.is_empty() {
            shards.push(Shard(current_content));
        }
        shards
    }

    pub fn shard(&self, contents: Vec<String>) -> Vec<Shard> {
        match self {
            Sharding::Token(max_tokens) => Self::shard_by_fn(*max_tokens, contents, count_tokens),
            Sharding::Byte(max_tokens) => Self::shard_by_fn(*max_tokens, contents, count_tokens),
        }
    }
}

impl From<ShardingType> for Sharding {
    fn from(sharding_type: ShardingType) -> Self {
        match sharding_type {
            ShardingType::Token(limit) => Sharding::Token(limit),
            ShardingType::Byte(limit) => Sharding::Byte(limit),
        }
    }
}

impl From<ShardingConfig> for Sharding {
    fn from(config: ShardingConfig) -> Self {
        config.sharding_type.into()
    }
}

impl From<IndexingConfig> for Config {
    fn from(config: IndexingConfig) -> Self {
        Config {
            sharding: config.sharding.map(|s| s.into()),
            ignore_patterns: if config.ignore_patterns.is_empty() {
                None
            } else {
                Some(config.ignore_patterns)
            },
        }
    }
}

pub struct ForgeIndex {
    config: Config,
    cwd: PathBuf,
}

impl ForgeIndex {
    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    pub fn with_indexing_config(mut self, config: IndexingConfig) -> Self {
        self.config = config.into();
        self
    }

    pub fn from_path(path: PathBuf) -> Self {
        Self { cwd: path, config: Default::default() }
    }
}

#[async_trait::async_trait]
impl IndexCodebaseService for ForgeIndex {
    async fn index(
        &self,
        target_directories: Option<Vec<String>>,
    ) -> anyhow::Result<CodebaseIndexOutput> {
        // Configure yek with the provided options
        let mut config = yek::config::YekConfig {
            input_paths: target_directories.unwrap_or(vec![self.cwd.display().to_string()]),
            ..Default::default()
        };

        // Apply ignore patterns from configuration
        if let Some(ignore_patterns) = &self.config.ignore_patterns {
            config.ignore_patterns.extend(ignore_patterns.clone());
        }

        // Use yek to process the repository
        let (_, processed_files) = yek::serialize_repo(&config)
            .with_context(|| format!("Failed to index paths: {:?}", config.input_paths))?;

        // perform sharding
        let shards = if let Some(sharding) = &self.config.sharding {
            sharding.shard(
                processed_files
                    .into_iter()
                    .map(|file| {
                        format!(
                            "<file path=\"{}\">\n{}\n</file>",
                            file.rel_path, file.content
                        )
                    })
                    .collect(),
            )
        } else {
            vec![Shard(
                processed_files
                    .into_iter()
                    .map(|file| {
                        format!(
                            "<file path=\"{}\">\n{}\n</file>",
                            file.rel_path, file.content
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            )]
        };

        Ok(CodebaseIndexOutput { shards })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;
    use tokio::fs;

    use super::*;

    // Helper function to compare shards by their content
    fn assert_shards_eq(actual: &[Shard], expected: &[Shard]) {
        assert_eq!(actual.len(), expected.len(), "Shard count mismatch");
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_eq!(a.0, e.0, "Shard {} content mismatch", i);
        }
    }

    #[test]
    fn test_shard_by_fn_empty_contents() {
        let fixture = Vec::new();
        let actual = Sharding::shard_by_fn(100, fixture, |s| s.len());
        let expected: Vec<Shard> = vec![];
        assert_shards_eq(&actual, &expected);
    }

    #[test]
    fn test_shard_by_fn_single_content() {
        let fixture = vec!["hello".to_string()];
        let actual = Sharding::shard_by_fn(100, fixture, |s| s.len());
        let expected = vec![Shard("hello".to_string())];
        assert_shards_eq(&actual, &expected);
    }

    #[test]
    fn test_shard_by_fn_multiple_contents_within_limit() {
        let fixture = vec!["hello".to_string(), "world".to_string()];
        let actual = Sharding::shard_by_fn(100, fixture, |s| s.len());
        let expected = vec![Shard("hello\nworld".to_string())];
        assert_shards_eq(&actual, &expected);
    }

    #[test]
    fn test_shard_by_fn_multiple_contents_exceeding_limit() {
        let fixture = vec!["a".repeat(60), "b".repeat(60)];
        let actual = Sharding::shard_by_fn(100, fixture, |s| s.len());
        let expected = vec![Shard("a".repeat(60)), Shard("b".repeat(60))];
        assert_shards_eq(&actual, &expected);
    }

    #[test]
    fn test_sharding_token_limit() {
        let fixture = Sharding::Token(10000);
        let contents = vec!["short content".to_string(), "another piece".to_string()];
        let actual = fixture.shard(contents.clone());

        // With large limit, should be one shard
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].0, contents.join("\n"));
    }

    #[tokio::test]
    async fn test_forge_index_index_with_files() -> anyhow::Result<()> {
        let fixture_dir = TempDir::new()?;
        let fixture_file_path = fixture_dir.path().join("test.txt");
        fs::write(&fixture_file_path, "test content").await?;

        let fixture_index = ForgeIndex::from_path(fixture_dir.path().to_path_buf());
        let actual = fixture_index.index(None).await?;

        assert!(!actual.shards.is_empty());
        let combined_content = actual
            .shards
            .iter()
            .map(|s| s.0.as_str())
            .collect::<Vec<_>>()
            .join("");
        assert!(combined_content.contains("test content"));

        Ok(())
    }

    #[tokio::test]
    async fn test_forge_index_index_with_sharding() -> anyhow::Result<()> {
        let fixture_dir = TempDir::new()?;
        let test = fixture_dir.path().join("test.txt");
        fs::write(&test, "test content for sharding").await?;
        let test2 = fixture_dir.path().join("test2.txt");
        fs::write(&test2, "test2 content for sharding").await?;

        let fixture_config = Config { sharding: Some(Sharding::Token(100)), ignore_patterns: None };
        let fixture_index =
            ForgeIndex::from_path(fixture_dir.path().to_path_buf()).with_config(fixture_config);
        let actual = fixture_index.index(None).await?;
        assert_eq!(actual.shards.len(), 1);
        Ok(())
    }
}
