use anyhow::Result;
use forge_app::UserInfra;
use forge_select::ForgeSelect;

pub struct ForgeInquire;

impl Default for ForgeInquire {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper that ensures cursor is shown on Ctrl+C
pub fn with_cursor_restore<F, T>(f: F) -> anyhow::Result<T>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    let _handle = tokio::spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            use crossterm::{cursor::Show, execute};
            let _ = execute!(std::io::stdout(), Show);
            std::process::exit(130);
        }
    });
    f()
}

impl ForgeInquire {
    pub fn new() -> Self {
        Self
    }

    async fn prompt<T, F>(&self, f: F) -> Result<Option<T>>
    where
        F: FnOnce() -> Result<Option<T>> + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::spawn_blocking(|| with_cursor_restore(f)).await?
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
