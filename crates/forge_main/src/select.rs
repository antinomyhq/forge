use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Confirm, Select};

/// Centralized dialoguer select functionality with consistent error handling
pub struct ForgeSelect;

/// Builder for select prompts
pub struct SelectBuilder<T> {
    message: String,
    options: Vec<T>,
    starting_cursor: Option<usize>,
    default: Option<bool>,
    help_message: Option<&'static str>,
}

impl ForgeSelect {
    /// Create a consistent theme for all select operations
    fn default_theme() -> ColorfulTheme {
        ColorfulTheme::default()
    }

    /// Entry point for select operations
    pub fn select<T>(message: impl Into<String>, options: Vec<T>) -> SelectBuilder<T> {
        SelectBuilder {
            message: message.into(),
            options,
            starting_cursor: None,
            default: None,
            help_message: None,
        }
    }

    /// Convenience method for confirm (yes/no)
    pub fn confirm(message: impl Into<String>) -> SelectBuilder<bool> {
        SelectBuilder {
            message: message.into(),
            options: vec![true, false],
            starting_cursor: None,
            default: None,
            help_message: None,
        }
    }
}

impl<T: 'static> SelectBuilder<T> {
    /// Set starting cursor position
    pub fn with_starting_cursor(mut self, cursor: usize) -> Self {
        self.starting_cursor = Some(cursor);
        self
    }

    /// Set default for confirm (only works with bool options)
    pub fn with_default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }

    /// Set help message
    pub fn with_help_message(mut self, message: &'static str) -> Self {
        self.help_message = Some(message);
        self
    }

    /// Execute select prompt
    pub fn prompt(self) -> Result<Option<T>>
    where
        T: std::fmt::Display + Clone,
    {
        // Handle confirm case (bool options)
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<bool>() {
            let theme = ForgeSelect::default_theme();
            let mut confirm = Confirm::with_theme(&theme)
                .with_prompt(&self.message);

            if let Some(default) = self.default {
                confirm = confirm.default(default);
            }

            let result = confirm.interact_opt().map_err(anyhow::Error::from)?;
            // Safe cast since we checked the type
            return Ok(result.map(|b| unsafe { std::mem::transmute_copy(&b) }));
        }

        // Regular select
        if self.options.is_empty() {
            return Ok(None);
        }

        let theme = ForgeSelect::default_theme();
        let mut select = Select::with_theme(&theme)
            .with_prompt(&self.message)
            .items(&self.options);

        if let Some(cursor) = self.starting_cursor {
            select = select.default(cursor);
        } else {
            select = select.default(0);
        }

        let idx_opt = select.interact_opt().map_err(anyhow::Error::from)?;
        Ok(idx_opt.and_then(|idx| self.options.get(idx).cloned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_select_builder_creates() {
        let builder = ForgeSelect::select("Test", vec!["a", "b", "c"]);
        assert_eq!(builder.message, "Test");
        assert_eq!(builder.options, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_confirm_builder_creates() {
        let builder = ForgeSelect::confirm("Confirm?");
        assert_eq!(builder.message, "Confirm?");
        assert_eq!(builder.options, vec![true, false]);
    }
}
