use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use forge_api::Provider;
use forge_app::{EnvironmentService, FileReadService, Infrastructure};
use forge_domain::Environment;

// data structure for implementing methods for foreign types.
struct ForeignTypeImpl<T>(T);

impl ForeignTypeImpl<Provider> {
    // maps environment variables to provider
    fn from_env_var(var: &str) -> anyhow::Result<Provider> {
        match var {
            "OPEN_ROUTER_KEY" => Ok(Provider::OpenRouter),
            "OPEN_AI_KEY" => Ok(Provider::OpenAI),
            "ANTHROPIC_KEY" => Ok(Provider::Anthropic),
            "FORGE_KEY" => {
                let provider_url = std::env::var("FORGE_PROVIDER_URL").map_err(|_| anyhow::anyhow!("FORGE_PROVIDER_URL must be set for FORGE_KEY"))?;
                Provider::from_url(&provider_url)
            }
            _ => Err(anyhow::anyhow!("No provider key found, please set one of: FORGE_KEY, OPEN_ROUTER_KEY, OPEN_AI_KEY or ANTHROPIC_KEY")),
        }
    }
}

pub struct TestFileReadService;

#[async_trait::async_trait]
impl FileReadService for TestFileReadService {
    async fn read(&self, path: &Path) -> Result<String> {
        Ok(tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read file: {}", path.display()))?)
    }
}

pub struct TestEnvironmentService {
    restricted: bool,
    provider_env_name: String,
}

impl TestEnvironmentService {
    pub fn new(restricted: bool, provider_env_name: String) -> Self {
        Self { restricted, provider_env_name }
    }

    /// Get path to appropriate shell based on platform and mode
    fn get_shell_path(&self) -> String {
        if cfg!(target_os = "windows") {
            std::env::var("COMSPEC").unwrap_or("cmd.exe".to_string())
        } else if self.restricted {
            // Default to rbash in restricted mode
            "/bin/rbash".to_string()
        } else {
            // Use user's preferred shell or fallback to sh
            std::env::var("SHELL").unwrap_or("/bin/sh".to_string())
        }
    }

    pub fn get(&self) -> Environment {
        dotenv::dotenv().ok();
        let cwd = std::env::current_dir().unwrap_or(PathBuf::from("."));

        // get provider url from environment variable
        let provider =
            ForeignTypeImpl::<Provider>::from_env_var(self.provider_env_name.as_str()).unwrap();
        let provider_key = provider
            .to_key()
            .unwrap_or_else(|| panic!("Failed to get provider key for {}", self.provider_env_name));
        let provider_url = provider.to_base_url().to_string();

        Environment {
            os: std::env::consts::OS.to_string(),
            cwd,
            shell: self.get_shell_path(),
            base_path: dirs::config_dir()
                .map(|a| a.join("forge"))
                .unwrap_or(PathBuf::from(".").join(".forge")),
            home: dirs::home_dir(),
            provider_key,
            provider_url,
        }
    }
}

impl EnvironmentService for TestEnvironmentService {
    fn get_environment(&self) -> forge_domain::Environment {
        self.get()
    }
}

pub struct TestInfra {
    file_read_service: TestFileReadService,
    environment_service: TestEnvironmentService,
}

impl TestInfra {
    pub fn new(restricted: bool, provider_env_name: String) -> Self {
        Self {
            file_read_service: TestFileReadService,
            environment_service: TestEnvironmentService::new(restricted, provider_env_name),
        }
    }
}

impl Infrastructure for TestInfra {
    type EnvironmentService = TestEnvironmentService;
    type FileReadService = TestFileReadService;

    fn environment_service(&self) -> &Self::EnvironmentService {
        &self.environment_service
    }

    fn file_read_service(&self) -> &Self::FileReadService {
        &self.file_read_service
    }
}
