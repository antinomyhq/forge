use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect, Select};
use forge_services::UserInfra;

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

    fn theme() -> ColorfulTheme {
        ColorfulTheme::default()
    }

    async fn prompt<T, F>(&self, f: F) -> Result<Option<T>>
    where
        F: FnOnce() -> std::io::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let result = tokio::task::spawn_blocking(f).await?;

        match result {
            Ok(value) => Ok(Some(value)),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[async_trait::async_trait]
impl UserInfra for ForgeInquire {
    async fn prompt_question(&self, question: &str) -> Result<Option<String>> {
        let question = question.to_string();
        self.prompt(move || {
            let result = Input::with_theme(&Self::theme())
                .with_prompt(&question)
                .allow_empty(true)
                .interact_text()?;
            Ok(result)
        })
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
        self.prompt(move || {
            let selection = Select::with_theme(&Self::theme())
                .with_prompt(&message)
                .items(&options)
                .default(0)
                .interact_opt()?;
            
            Ok(selection.map(|idx| options.into_iter().nth(idx).unwrap()))
        })
        .await
        .map(|opt| opt.flatten())
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
        self.prompt(move || {
            let selections = MultiSelect::with_theme(&Self::theme())
                .with_prompt(&message)
                .items(&options)
                .interact_opt()?;
            
            Ok(selections.map(|indices| {
                indices
                    .into_iter()
                    .filter_map(|idx| options.get(idx).cloned())
                    .collect()
            }))
        })
        .await
        .map(|opt| opt.flatten())
    }
}
