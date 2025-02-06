use std::sync::Arc;

use anyhow::Result;
use forge_domain::{
    ChatRequest, Environment, FileReadService, ProviderService, SystemContext, ToolService,
};
use forge_walker::Walker;
use handlebars::Handlebars;
use tracing::debug;

use super::{PromptService, Service};

impl Service {
    pub fn system_prompt(
        env: Environment,
        tool: Arc<dyn ToolService>,
        provider: Arc<dyn ProviderService>,
        file_read: Arc<dyn FileReadService>,
    ) -> impl PromptService {
        Live::new(env, tool, provider, file_read)
    }
}

#[derive(Clone)]
struct Live {
    env: Environment,
    tool: Arc<dyn ToolService>,
    provider: Arc<dyn ProviderService>,
    file_read: Arc<dyn FileReadService>,
}

impl Live {
    pub fn new(
        env: Environment,
        tool: Arc<dyn ToolService>,
        provider: Arc<dyn ProviderService>,
        file_read: Arc<dyn FileReadService>,
    ) -> Self {
        Self { env, tool, provider, file_read }
    }
}

#[async_trait::async_trait]
impl PromptService for Live {
    async fn get(&self, request: &ChatRequest) -> Result<String> {
        let template = include_str!("../prompts/coding/system.md");

        let custom_instructions = match request.custom_instructions {
            None => None,
            Some(ref path) => {
                let content = self.file_read.read(path.clone()).await.unwrap();
                Some(content)
            }
        };

        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(|str| str.to_string());

        let tool_supported = self
            .provider
            .parameters(&request.model)
            .await?
            .tool_supported;

        debug!(
            "Tool support for {}: {}",
            request.model.as_str(),
            tool_supported
        );

        let mut files = Walker::max_all()
            .max_depth(2)
            .cwd(self.env.cwd.clone())
            .get()
            .await?
            .iter()
            .map(|f| f.path.to_string())
            .collect::<Vec<_>>();

        // Sort the files alphabetically to ensure consistent ordering
        files.sort();

        let ctx = SystemContext {
            env: self.env.clone(),
            tool_information: self.tool.usage_prompt(),
            tool_supported,
            custom_instructions,
            files,
        };

        Ok(hb.render_template(template, &ctx)?)
    }
}

#[cfg(test)]
mod tests {

    use std::path::PathBuf;

    use forge_domain::{ModelId, Parameters};
    use insta::assert_snapshot;
    use tempfile::TempDir;
    use tokio::fs;

    use super::*;
    use crate::service::test::{TestFileReadService, TestProvider};

    async fn test_env(dir: PathBuf) -> Environment {
        fs::write(dir.join("file1.txt"), "A").await.unwrap();
        fs::write(dir.join("file2.txt"), "B").await.unwrap();
        fs::create_dir_all(dir.join("nested")).await.unwrap();
        fs::write(dir.join("nested").join("file3.txt"), "B")
            .await
            .unwrap();
        Environment {
            os: "linux".to_string(),
            cwd: dir,
            shell: "/bin/bash".to_string(),
            api_key: Some("test".to_string()),
            large_model_id: "open-ai/gpt-4o".to_string(),
            small_model_id: "open-ai/gpt-4o-mini".to_string(),
            base_path: PathBuf::from("/home/user/.forge/globalConfig"),
            base_url: None,
            home: Some(PathBuf::from("/home/user")),
            model: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_tool_supported() {
        let dir = TempDir::new().unwrap();
        let env = test_env(dir.path().to_path_buf()).await;
        let tools = Arc::new(Service::tool_service());
        let provider = Arc::new(
            TestProvider::default()
                .parameters(vec![(ModelId::new("gpt-3.5-turbo"), Parameters::new(true))]),
        );
        let file = Arc::new(TestFileReadService::default());
        let request = ChatRequest::new(ModelId::new("gpt-3.5-turbo"), "test task");
        let prompt = Live::new(env, tools, provider, file)
            .get(&request)
            .await
            .unwrap()
            .replace(&dir.path().display().to_string(), "[TEMP_DIR]");

        assert_snapshot!(prompt);
    }

    #[tokio::test]
    async fn test_tool_unsupported() {
        let dir = TempDir::new().unwrap();
        let env = test_env(dir.path().to_path_buf()).await;
        let tools = Arc::new(Service::tool_service());
        let provider = Arc::new(TestProvider::default().parameters(vec![(
            ModelId::new("gpt-3.5-turbo"),
            Parameters::new(false),
        )]));
        let file = Arc::new(TestFileReadService::default());
        let request = ChatRequest::new(ModelId::new("gpt-3.5-turbo"), "test task");
        let prompt = Live::new(env, tools, provider, file)
            .get(&request)
            .await
            .unwrap()
            .replace(&dir.path().display().to_string(), "[TEMP_DIR]");
        assert_snapshot!(prompt);
    }

    #[tokio::test]
    async fn test_system_prompt_custom_prompt() {
        let dir = TempDir::new().unwrap();
        let env = test_env(dir.path().to_path_buf()).await;
        let tools = Arc::new(Service::tool_service());
        let provider = Arc::new(TestProvider::default().parameters(vec![(
            ModelId::new("gpt-3.5-turbo"),
            Parameters::new(false),
        )]));
        let file = Arc::new(TestFileReadService::default().add(".custom.md", "Woof woof!"));
        let request = ChatRequest::new(ModelId::new("gpt-3.5-turbo"), "test task")
            .custom_instructions(".custom.md");
        let prompt = Live::new(env, tools, provider, file)
            .get(&request)
            .await
            .unwrap()
            .replace(&dir.path().display().to_string(), "[TEMP_DIR]");
        assert!(prompt.contains("Woof woof!"));
    }
}
