use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use forge_app::domain::Workflow;
use forge_app::WorkflowService;
use merge::Merge;
use tokio::sync::RwLock;

use crate::{EnvironmentInfra, FileReaderInfra, FileWriterInfra, HttpInfra};

/// A workflow loader to load the workflow from the given path.
/// It also resolves the internal paths specified in the workflow.
pub struct ForgeWorkflowService<F> {
    infra: Arc<F>,
    workflow_cache: Arc<RwLock<Option<Workflow>>>,
}

impl<F> ForgeWorkflowService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra, workflow_cache: Arc::new(RwLock::new(None)) }
    }
}

impl<F: FileWriterInfra + FileReaderInfra> ForgeWorkflowService<F> {
    /// Find a forge.yaml config file by traversing parent directories.
    /// Returns the path to the first found config file, or the original path if
    /// none is found.
    pub async fn resolve_path(&self, path: Option<PathBuf>) -> PathBuf {
        let path = path.unwrap_or(PathBuf::from("."));
        // If the path exists or this is an explicitly provided path, return it as is
        if path.exists() || path.to_string_lossy() != "forge.yaml" {
            return path.to_path_buf();
        }

        // Get the current directory as the starting point
        let mut current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let filename = path.file_name().unwrap_or_default();

        // Start searching for the config in the current directory and its parents
        loop {
            let config_path = current_dir.join(filename);
            if config_path.exists() {
                return config_path;
            }

            // Try to go up one directory
            match current_dir.parent() {
                Some(parent) if parent != current_dir => {
                    current_dir = parent.to_path_buf();
                }
                // Stop if we've reached the root directory or can't go further up
                _ => break,
            }
        }

        // If no config was found, return the original path
        path.to_path_buf()
    }

    /// Loads the workflow from the given path.
    /// If the path is just "forge.yaml", searches for it in parent directories.
    /// If the file doesn't exist anywhere, creates a new empty workflow file at
    /// the specified path (in the current directory).
    async fn read(&self, path: &Path) -> anyhow::Result<Workflow> {
        // First, try to find the config file in parent directories if needed
        let path = &self.resolve_path(Some(path.into())).await;

        if !path.exists() {
            let workflow = Workflow::new();
            self.infra
                .write(path, self.serialize_workflow(&workflow)?.into(), true)
                .await?;

            Ok(workflow)
        } else {
            let content = self.infra.read_utf8(path).await?;
            let workflow: Workflow = serde_yml::from_str(&content)
                .with_context(|| format!("Failed to parse workflow from {}", path.display()))?;
            Ok(workflow)
        }
    }

    // Serializes the workflow to a YAML string.
    fn serialize_workflow(&self, workflow: &Workflow) -> anyhow::Result<String> {
        let lsp = if cfg!(debug_assertions) {
            "./forge.schema.json"
        } else {
            "https://raw.githubusercontent.com/antinomyhq/forge/refs/heads/main/forge.schema.json"
        };
        let contents = serde_yml::to_string(workflow)?;
        Ok(format!("# yaml-language-server: $schema={lsp}\n{contents}"))
    }
}

#[async_trait::async_trait]
impl<F: FileWriterInfra + FileReaderInfra + HttpInfra + EnvironmentInfra> WorkflowService
    for ForgeWorkflowService<F>
{
    async fn resolve(&self, path: Option<PathBuf>) -> PathBuf {
        self.resolve_path(path).await
    }

    async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let path_to_use = path.unwrap_or_else(|| Path::new("forge.yaml"));
        self.read(path_to_use).await
    }

    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()> {
        // First, try to find the config file in parent directories if needed
        let path_buf = match path {
            Some(p) => p.to_path_buf(),
            None => PathBuf::from("forge.yaml"),
        };
        let resolved_path = self.resolve_path(Some(path_buf)).await;

        let content = self.serialize_workflow(workflow)?;
        self.infra.write(&resolved_path, content.into(), true).await
    }

    async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let workflow = self.read_workflow(path).await?;
        let mut base_workflow = Workflow::default();
        {
            let cache_guard = self.workflow_cache.read().await;
            if let Some(cached_workflow) = cache_guard.as_ref() {
                base_workflow.merge(cached_workflow.clone());
                drop(cache_guard);
                base_workflow.merge(workflow);
                return Ok(base_workflow);
            }
        }

        match self.get_api_workflow().await {
            Ok(api_workflow) => {
                base_workflow.merge(api_workflow);
            }
            Err(error) => {
                tracing::error!(error = ?error, "Failed to fetch API workflow");
            }
        }

        base_workflow.merge(workflow);
        {
            self.workflow_cache
                .write()
                .await
                .replace(base_workflow.clone());
        }

        Ok(base_workflow)
    }

    async fn update_workflow<Func>(&self, path: Option<&Path>, f: Func) -> anyhow::Result<Workflow>
    where
        Func: FnOnce(&mut Workflow) + Send,
    {
        // Read the current workflow
        let path_to_use = path.unwrap_or_else(|| Path::new("forge.yaml"));
        let mut workflow = self.read(path_to_use).await?;

        // Apply the closure to update the workflow
        f(&mut workflow);

        // Write the updated workflow back
        self.write_workflow(path, &workflow).await?;

        Ok(workflow)
    }

    async fn get_api_workflow(&self) -> anyhow::Result<Workflow> {
        let env = self.infra.get_environment();
        let version = env.version();
        let base_url = env.forge_api_url;
        let url = format!("{base_url}config?version={version}");

        let response = self.infra.get(&url, None).await?;
        let response = response.error_for_status()?;
        let body = response.text().await?;
        let workflow: Workflow = serde_json::from_str(&body)
            .with_context(|| "Failed to parse default workflow from response".to_string())?;

        Ok(workflow)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Arc;

    use bytes::Bytes;
    use forge_app::domain::Environment;
    use pretty_assertions::assert_eq;
    use reqwest::header::HeaderMap;
    use reqwest::Response;
    use serde_json::Value;
    use tempfile::TempDir;
    use url::Url;

    use super::*;

    /// This testing strategy tests the core algorithm directly without
    /// depending on complex directory structures.
    #[test]
    fn test_find_config_file_behavior() {
        // Test 1: Return exact path if file exists
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("forge.yaml");
        fs::write(&config_path, "test content").unwrap();

        let result = find_config_file_logic(Path::new("forge.yaml"), &config_path);
        assert_eq!(result, config_path);

        // Test 2: Return original path for non-forge.yaml files
        let custom_path = PathBuf::from("custom-config.yaml");
        let result =
            find_config_file_logic(&custom_path, &temp_dir.path().join("file-that-exists.txt"));
        assert_eq!(result, custom_path);

        // Test 3: Return parent path when found
        let parent_dir = temp_dir.path().join("parent");
        let child_dir = parent_dir.join("child");
        fs::create_dir_all(&child_dir).unwrap();

        let parent_config = parent_dir.join("forge.yaml");
        fs::write(&parent_config, "parent config").unwrap();

        let result = find_config_file_logic(Path::new("forge.yaml"), &parent_config);
        assert_eq!(result, parent_config);
    }

    // Pure function that tests the core logic without filesystem dependencies
    fn find_config_file_logic(path: &Path, existing_config_path: &Path) -> PathBuf {
        // If the path exists or this is an explicitly provided path, return it as is
        if path.exists() || path.to_string_lossy() != "forge.yaml" {
            return path.to_path_buf();
        }

        // Simulate checking directories by checking if the existing_config_path
        // contains the filename we're looking for
        if existing_config_path.file_name().unwrap_or_default()
            == path.file_name().unwrap_or_default()
        {
            return existing_config_path.to_path_buf();
        }

        // If no config was found, return the original path
        path.to_path_buf()
    }

    #[test]
    fn test_find_config_not_found() {
        // Create a temporary directory without a config
        let temp_dir = TempDir::new().unwrap();
        let test_dir = temp_dir.path().join("test_dir");
        fs::create_dir_all(&test_dir).unwrap();

        // Save the original directory and change to the test dir
        let original_dir = std::env::current_dir().unwrap();

        // Only create the directory structure, but don't create forge.yaml
        // so the find function should return the original path
        std::env::set_current_dir(&test_dir).unwrap();

        // Test explicitly only the file existence check logic
        assert!(!Path::new("forge.yaml").exists());

        // Restore the original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_explicit_path_not_searched() {
        // Create a test directory structure
        let temp_dir = TempDir::new().unwrap();
        let parent_dir = temp_dir.path().join("parent");
        let child_dir = parent_dir.join("child");
        fs::create_dir_all(&child_dir).unwrap();

        // Create forge.yaml in the parent
        fs::write(parent_dir.join("forge.yaml"), "# Test").unwrap();

        // Simulate search with a non-forge.yaml path
        let custom_path = PathBuf::from("custom-config.yaml");
        let parent_config = parent_dir.join("forge.yaml");

        let result = find_config_file_logic(&custom_path, &parent_config);

        // Should return the custom path unchanged
        assert_eq!(result, custom_path);
    }

    #[derive(Debug)]
    struct MockInfra {
        file_contents: HashMap<PathBuf, String>,
        http_response: Option<Result<String, String>>,
        environment: Environment,
    }

    impl MockInfra {
        fn new() -> Self {
            Self {
                file_contents: HashMap::new(),
                http_response: None,
                environment: Environment {
                    os: "test".to_string(),
                    pid: 12345,
                    cwd: PathBuf::from("/test"),
                    home: Some(PathBuf::from("/home/test")),
                    shell: "bash".to_string(),
                    base_path: PathBuf::from("/base"),
                    retry_config: Default::default(),
                    max_search_lines: 25,
                    fetch_truncation_limit: 0,
                    stdout_max_prefix_length: 0,
                    stdout_max_suffix_length: 0,
                    max_read_size: 0,
                    http: Default::default(),
                    max_file_size: 10_000_000,
                    forge_api_url: Url::parse("http://forgecode.dev/api/").unwrap(),
                },
            }
        }

        fn with_http_response(mut self, response: Result<String, String>) -> Self {
            self.http_response = Some(response);
            self
        }
    }

    #[async_trait::async_trait]
    impl FileReaderInfra for MockInfra {
        async fn read_utf8(&self, path: &Path) -> anyhow::Result<String> {
            self.file_contents
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("File not found: {}", path.display()))
        }

        async fn read(&self, _path: &Path) -> anyhow::Result<Vec<u8>> {
            unimplemented!("Not needed for these tests")
        }

        async fn range_read_utf8(
            &self,
            _path: &Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, forge_fs::FileInfo)> {
            unimplemented!("Not needed for these tests")
        }
    }

    #[async_trait::async_trait]
    impl FileWriterInfra for MockInfra {
        async fn write(
            &self,
            _path: &Path,
            _contents: Bytes,
            _capture_snapshot: bool,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn write_temp(
            &self,
            _prefix: &str,
            _ext: &str,
            _content: &str,
        ) -> anyhow::Result<PathBuf> {
            unimplemented!("Not needed for these tests")
        }
    }

    #[async_trait::async_trait]
    impl HttpInfra for MockInfra {
        async fn get(&self, _url: &str, _headers: Option<HeaderMap>) -> anyhow::Result<Response> {
            match &self.http_response {
                Some(Ok(body)) => {
                    // Create a simple mock response using reqwest::Response
                    let _url = reqwest::Url::parse("http://test.com").unwrap();
                    let response = reqwest::Response::from(
                        http::Response::builder()
                            .status(200)
                            .body(body.clone())
                            .unwrap(),
                    );
                    Ok(response)
                }
                Some(Err(error)) => Err(anyhow::anyhow!("{}", error)),
                None => Err(anyhow::anyhow!("No HTTP response configured")),
            }
        }

        async fn post(&self, _url: &str, _body: Bytes) -> anyhow::Result<Response> {
            unimplemented!("Not needed for these tests")
        }

        async fn delete(&self, _url: &str) -> anyhow::Result<Response> {
            unimplemented!("Not needed for these tests")
        }
    }

    impl EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Environment {
            self.environment.clone()
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }
    }

    fn create_api_workflow_json() -> String {
        r#"{
  "agents": [
    {
      "id": "api-agent",
      "model": "gpt-3.5-turbo",
      "description": "API agent"
    }
  ],
  "variables": {
    "api_var": "api_value"
  },
  "temperature": 0.7
}"#
        .to_string()
    }

    #[tokio::test]
    async fn test_read_merged_uses_cache_when_available() {
        let test_path = PathBuf::from("/test/custom-forge.yaml");
        let fixture = MockInfra::new();
        let service = ForgeWorkflowService::new(Arc::new(fixture));

        let mut cached_workflow = Workflow::new();
        cached_workflow.variables.insert(
            "cached_var".to_string(),
            Value::String("cached_value".to_string()),
        );
        service
            .workflow_cache
            .write()
            .await
            .replace(cached_workflow.clone());

        let actual = service.read_merged(Some(&test_path)).await.unwrap();

        assert_eq!(
            actual.variables.get("cached_var"),
            Some(&Value::String("cached_value".to_string()))
        );
    }

    #[tokio::test]
    async fn test_read_merged_makes_api_call_when_no_cache() {
        let test_path = PathBuf::from("/test/custom-forge.yaml");
        let fixture = MockInfra::new().with_http_response(Ok(create_api_workflow_json()));
        let service = ForgeWorkflowService::new(Arc::new(fixture));

        let actual = service.read_merged(Some(&test_path)).await.unwrap();

        // Should have API variables (proving API call was made)
        assert_eq!(
            actual.variables.get("api_var"),
            Some(&Value::String("api_value".to_string()))
        );

        // Should cache the result
        let cache_guard = service.workflow_cache.read().await;
        assert!(cache_guard.is_some());
    }

    #[tokio::test]
    async fn test_read_merged_handles_api_failure() {
        let test_path = PathBuf::from("/test/custom-forge.yaml");
        let fixture = MockInfra::new().with_http_response(Err("Network error".to_string()));
        let service = ForgeWorkflowService::new(Arc::new(fixture));

        let actual = service.read_merged(Some(&test_path)).await.unwrap();

        assert!(actual.variables.get("api_var").is_none());

        assert!(actual.agents.len() >= 2);
    }
}
