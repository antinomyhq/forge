use std::sync::Arc;

use anyhow::Result;
use forge_app::UserInfra;
use forge_select::ForgeSelect;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ForgeInquire {
    // Mutex to ensure that only one prompt is active at a time
    ready: Arc<Mutex<()>>,
}

impl Default for ForgeInquire {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeInquire {
    pub fn new() -> Self {
        Self { ready: Arc::new(Mutex::new(())) }
    }

    async fn prompt<T, F>(&self, f: F) -> Result<Option<T>>
    where
        F: FnOnce() -> Result<Option<T>> + Send + 'static,
        T: Send + 'static,
    {
        let ready = self.ready.lock().await;
        let result = tokio::task::spawn_blocking(f).await?;
        drop(ready);
        result
    }
}

#[async_trait::async_trait]
impl UserInfra for ForgeInquire {
    async fn prompt_question(&self, question: &str) -> Result<Option<String>> {
        let question = question.to_string();
        self.prompt(move || ForgeSelect::input(&question).allow_empty(true).prompt())
            .await
    }

    async fn select_one<T: std::fmt::Display + Send + 'static>(
        &self,
        message: &str,
        options: Vec<T>,
    ) -> Result<Option<T>> {
        if options.is_empty() {
            return Ok(None);
        }

        let message = message.to_string();
        self.prompt(move || ForgeSelect::select_owned(&message, options).prompt())
            .await
    }

    async fn select_many<T: std::fmt::Display + Clone + Send + 'static>(
        &self,
        message: &str,
        options: Vec<T>,
    ) -> Result<Option<Vec<T>>> {
        if options.is_empty() {
            return Ok(None);
        }

        let message = message.to_string();
        self.prompt(move || ForgeSelect::multi_select(&message, options).prompt())
            .await
    }
}
