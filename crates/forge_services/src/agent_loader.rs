use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::domain::{Agent, AgentId};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    DirectoryReaderInfra, EnvironmentInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra,
};

/// A service for loading agent definitions from individual files in the
/// forge/agent directory
pub struct AgentLoaderService<F> {
    infra: Arc<F>,

    // Cache is used to maintain the loaded agents
    // for this service instance.
    // So that they could live till user starts a new session.
    cache: Arc<Mutex<Option<Vec<Agent>>>>,
}

/// Represents the frontmatter structure of an agent definition file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentFrontmatter {
    #[serde(flatten)]
    agent: Agent,
}

impl<F> AgentLoaderService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, cache: Arc::new(Default::default()) }
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    forge_app::AgentLoaderService for AgentLoaderService<F>
{
    /// Load all agent definitions from the forge/agent directory
    async fn load_agents(&self) -> anyhow::Result<Vec<Agent>> {
        if let Some(agents) = self.cache.lock().await.as_ref() {
            return Ok(agents.clone());
        }
        let agent_dir = self.infra.get_environment().agent_path();
        if !self.infra.exists(&agent_dir).await? {
            return Ok(vec![]);
        }

        let mut agents = vec![];

        // Use DirectoryReaderInfra to read all .md files in parallel
        let files = self
            .infra
            .read_directory_files(&agent_dir, Some("*.md"))
            .await
            .with_context(|| "Failed to read agent directory")?;

        for (path, content) in files {
            // Extract filename (without extension) as agent ID
            let filename = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());

            agents.push(self.parse_agent_file(&content, filename).await?)
        }

        *self.cache.lock().await = Some(agents.clone());

        Ok(agents)
    }
}

impl<F: FileReaderInfra + FileWriterInfra + FileInfoInfra + EnvironmentInfra + DirectoryReaderInfra>
    AgentLoaderService<F>
{
    /// Parse raw content into an Agent with YAML frontmatter
    async fn parse_agent_file(&self, content: &str, agent_id: Option<String>) -> Result<Agent> {
        // Parse the frontmatter using gray_matter with type-safe deserialization
        let matter = Matter::<YAML>::new();
        let result = matter
            .parse::<AgentFrontmatter>(content)
            .with_context(|| "Failed to parse YAML frontmatter")?;

        // Extract the frontmatter
        let frontmatter = result.data.context("No YAML frontmatter found")?;

        // Use the provided agent_id or keep the existing one
        let mut agent = frontmatter.agent;
        if let Some(id) = agent_id {
            agent.id = AgentId::new(&id);
        }

        Ok(agent)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use forge_app::AgentLoaderService;
    use forge_app::domain::{Environment, HttpConfig, RetryConfig};
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;
    use url::Url;

    use super::*;

    // Mock infrastructure for testing
    #[derive(Clone)]
    struct MockInfra {
        environment: Environment,
        files: HashMap<PathBuf, String>,
    }

    impl MockInfra {
        fn new(environment: Environment) -> Self {
            Self { environment, files: HashMap::new() }
        }

        fn with_file(mut self, path: PathBuf, content: String) -> Self {
            self.files.insert(path, content);
            self
        }
    }

    #[async_trait::async_trait]
    impl EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Environment {
            self.environment.clone()
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }
    }

    #[async_trait::async_trait]
    impl FileReaderInfra for MockInfra {
        async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("File not found: {}", path.display()))
        }

        async fn read(&self, _path: &Path) -> anyhow::Result<Vec<u8>> {
            unimplemented!()
        }

        async fn range_read_utf8(
            &self,
            _path: &Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, forge_fs::FileInfo)> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FileWriterInfra for MockInfra {
        async fn write(
            &self,
            _path: &Path,
            _contents: bytes::Bytes,
            _capture_snapshot: bool,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn write_temp(
            &self,
            _prefix: &str,
            _ext: &str,
            _content: &str,
        ) -> anyhow::Result<PathBuf> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl FileInfoInfra for MockInfra {
        async fn is_binary(&self, _path: &Path) -> anyhow::Result<bool> {
            Ok(false)
        }

        async fn is_file(&self, _path: &Path) -> anyhow::Result<bool> {
            Ok(true)
        }

        async fn exists(&self, path: &Path) -> anyhow::Result<bool> {
            Ok(path == &self.environment.agent_path())
        }

        async fn file_size(&self, _path: &Path) -> anyhow::Result<u64> {
            Ok(0)
        }
    }

    #[async_trait::async_trait]
    impl DirectoryReaderInfra for MockInfra {
        async fn read_directory_files(
            &self,
            directory: &Path,
            pattern: Option<&str>,
        ) -> anyhow::Result<Vec<(PathBuf, String)>> {
            let mut files = Vec::new();

            for (path, content) in &self.files {
                if let Some(parent) = path.parent() {
                    if parent == directory {
                        // Apply filter if provided
                        if let Some(pattern) = pattern {
                            if pattern == "*.md" {
                                if let Some(extension) = path.extension() {
                                    if extension == "md" {
                                        files.push((path.clone(), content.clone()));
                                    }
                                }
                            }
                        } else {
                            files.push((path.clone(), content.clone()));
                        }
                    }
                }
            }

            Ok(files)
        }
    }

    #[tokio::test]
    async fn test_load_agents_with_directory_reader() {
        let temp_dir = tempdir().unwrap();

        let environment = Environment {
            os: "test".to_string(),
            pid: 12345,
            cwd: temp_dir.path().to_path_buf(),
            home: Some(temp_dir.path().to_path_buf()),
            shell: "bash".to_string(),
            base_path: temp_dir.path().to_path_buf(),
            retry_config: RetryConfig::default(),
            max_search_lines: 25,
            max_search_result_bytes: 256000,
            fetch_truncation_limit: 0,
            forge_api_url: Url::parse("http://localhost:8000").unwrap(),
            http: HttpConfig::default(),
            stdout_max_prefix_length: 50,
            stdout_max_suffix_length: 50,
            stdout_max_line_length: 200,
            max_read_size: 2000,
            max_file_size: 1024 * 1024,
        };

        let agent_dir = environment.agent_path();

        let agent_content = r#"---
id: "test"
title: "Test Agent"
description: "A test agent"
system_prompt: "Test instructions"
---

# Test Agent

This is the content of the test agent.
"#;

        let mock_infra = Arc::new(
            MockInfra::new(environment)
                .with_file(agent_dir.join("test.md"), agent_content.to_string()),
        );

        let service = super::AgentLoaderService::new(mock_infra);
        let actual = service.load_agents().await.unwrap();

        assert_eq!(actual.len(), 1);
        let agent = &actual[0];
        assert_eq!(agent.id.as_str(), "test");
        assert_eq!(agent.title.as_ref().unwrap(), "Test Agent");
        assert_eq!(agent.description.as_ref().unwrap(), "A test agent");
    }

    #[tokio::test]
    async fn test_load_agents_caches_results() {
        let temp_dir = tempdir().unwrap();
        let agent_dir = temp_dir.path().join("agents");

        let environment = Environment {
            os: "test".to_string(),
            pid: 12345,
            cwd: temp_dir.path().to_path_buf(),
            home: Some(temp_dir.path().to_path_buf()),
            shell: "bash".to_string(),
            base_path: temp_dir.path().to_path_buf(), /* Use temp_dir as base_path so
                                                       * agent_path() works */
            retry_config: RetryConfig::default(),
            max_search_lines: 25,
            max_search_result_bytes: 256000,
            fetch_truncation_limit: 0,
            forge_api_url: Url::parse("http://localhost:8000").unwrap(),
            http: HttpConfig::default(),
            stdout_max_prefix_length: 50,
            stdout_max_suffix_length: 50,
            stdout_max_line_length: 200,
            max_read_size: 2000,
            max_file_size: 1024 * 1024,
        };

        let mock_infra = Arc::new(
            MockInfra::new(environment).with_file(
                agent_dir.join("test.md"),
                r#"---
id: "test"
title: "Test Agent"
description: "A test agent"
system_prompt: "Test instructions"
---"#
                    .to_string(),
            ),
        );

        let service = super::AgentLoaderService::new(mock_infra);

        // First call should load from infra
        let first_result = service.load_agents().await.unwrap();

        // Second call should return cached result
        let second_result = service.load_agents().await.unwrap();

        assert_eq!(first_result.len(), second_result.len());
        assert_eq!(first_result[0].title, second_result[0].title);
    }
}
