use anyhow::Result;
use forge_services::UserInfra;
use inquire::ui::{RenderConfig, Styled};
use inquire::{InquireError, MultiSelect, Select, Text};

pub struct ForgeInquire;

impl Default for ForgeInquire {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeInquire {
    pub fn new() -> Self {
        Self
    }

    fn render_config() -> RenderConfig<'static> {
        RenderConfig::default()
            .with_scroll_up_prefix(Styled::new("⇡"))
            .with_scroll_down_prefix(Styled::new("⇣"))
            .with_highlighted_option_prefix(Styled::new("➤"))
    }

    async fn prompt<T, F>(&self, f: F) -> Result<Option<T>>
    where
        F: FnOnce() -> std::result::Result<T, InquireError> + Send + 'static,
        T: Send + 'static,
    {
        let result = tokio::task::spawn_blocking(f).await?;

        match result {
            Ok(value) => Ok(Some(value)),
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[async_trait::async_trait]
impl UserInfra for ForgeInquire {
    async fn prompt_question(&self, question: &str) -> Result<Option<String>> {
        let question = question.to_string();
        self.prompt(move || {
            Text::new(&question)
                .with_render_config(Self::render_config())
                .with_help_message("Press Enter to submit, ESC to cancel")
                .prompt()
        })
        .await
    }

    async fn select_one<T: std::fmt::Display + Send + 'static>(
        &self,
        message: &str,
        options: Vec<T>,
    ) -> Result<Option<T>> {
        let message = message.to_string();
        self.prompt(move || {
            Select::new(&message, options)
                .with_render_config(Self::render_config())
                .with_help_message("Use arrow keys to navigate, Enter to select, ESC to cancel")
                .prompt()
        })
        .await
    }

    async fn select_many<T: std::fmt::Display + Clone + Send + 'static>(
        &self,
        message: &str,
        options: Vec<T>,
    ) -> Result<Option<Vec<T>>> {
        let message = message.to_string();
        self.prompt(move || {
            MultiSelect::new(&message, options)
                .with_render_config(Self::render_config())
                .with_help_message("Use arrow keys to navigate, Space to select/deselect, Enter to confirm, ESC to cancel")
                .prompt()
        })
        .await
    }
}
