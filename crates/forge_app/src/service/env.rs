use std::path::PathBuf;

use anyhow::Result;
use forge_domain::Environment;
use forge_walker::Walker;
use tokio::sync::Mutex;

use super::Service;

#[async_trait::async_trait]
pub trait EnvironmentService {
    async fn get(&self) -> Result<Environment>;
}

impl Service {
    pub fn environment_service(base_dir: Option<PathBuf>) -> impl EnvironmentService {
        Live::new(base_dir)
    }
}

struct Live {
    env: Mutex<Option<Environment>>,
    base_dir: Option<PathBuf>,
}

impl Live {
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self { env: Mutex::new(None), base_dir }
    }

    async fn from_env(cwd: Option<PathBuf>) -> Result<Environment> {
        dotenv::dotenv().ok();
        let api_key = std::env::var("FORGE_KEY").expect("FORGE_KEY must be set");
        let large_model_id =
            std::env::var("FORGE_LARGE_MODEL").unwrap_or("anthropic/claude-3.5-sonnet".to_owned());
        let small_model_id =
            std::env::var("FORGE_SMALL_MODEL").unwrap_or("anthropic/claude-3.5-haiku".to_owned());

        let cwd = if let Some(cwd) = cwd {
            cwd
        } else {
            std::env::current_dir()?
        };

        let files = match Walker::min().cwd(cwd.clone()).max_depth(3).get().await {
            Ok(files) => files
                .into_iter()
                .filter(|f| !f.is_dir)
                .map(|f| f.path)
                .collect(),
            Err(_) => vec![],
        };

        Ok(Environment {
            os: std::env::consts::OS.to_string(),
            cwd: cwd.display().to_string(),
            shell: if cfg!(windows) {
                std::env::var("COMSPEC")?
            } else {
                std::env::var("SHELL").unwrap_or("/bin/sh".to_string())
            },
            home: dirs::home_dir().map(|a| a.display().to_string()),
            files,
            api_key,
            large_model_id,
            small_model_id,
            global_config: global_config_dir().await?,
        })
    }
}

async fn global_config_dir() -> Result<String> {
    let global_config = dirs::home_dir().ok_or(anyhow::anyhow!("Unable to get home dir."))?.join(".forge").join("globalConfig");
    tokio::fs::create_dir_all(&global_config).await?;
    Ok(global_config.display().to_string())
}

#[async_trait::async_trait]
impl EnvironmentService for Live {
    async fn get(&self) -> Result<Environment> {
        let mut guard = self.env.lock().await;

        if let Some(env) = guard.as_ref() {
            return Ok(env.clone());
        } else {
            *guard = Some(Live::from_env(self.base_dir.clone()).await?);
            Ok(guard.as_ref().unwrap().clone())
        }
    }
}
