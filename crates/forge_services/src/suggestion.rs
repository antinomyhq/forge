use std::sync::Arc;

use anyhow::Result;
use forge_domain::{EnvironmentService, File, SuggestionService};
use forge_walker::Walker;

use crate::Infrastructure;

pub struct ForgeSuggestionService<F> {
    domain: Arc<F>,
}

impl<F> ForgeSuggestionService<F> {
    pub fn new(domain: Arc<F>) -> Self {
        Self { domain }
    }
}

impl<F: Infrastructure> ForgeSuggestionService<F> {
    async fn get_suggestions(&self) -> Result<Vec<File>> {
        let cwd = self
            .domain
            .environment_service()
            .get_environment()
            .cwd
            .clone();
        let walker = Walker::max_all().cwd(cwd);

        let files = walker.get().await?;
        Ok(files
            .into_iter()
            .map(|file| File { path: file.path.clone(), is_dir: file.is_dir() })
            .collect())
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure + Send + Sync> SuggestionService for ForgeSuggestionService<F> {
    async fn suggestions(&self) -> Result<Vec<File>> {
        self.get_suggestions().await
    }
}
