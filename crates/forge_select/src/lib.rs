use anyhow::Result;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, MultiSelect, Select};

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

/// Builder for select prompts that takes ownership (doesn't require Clone)
pub struct SelectBuilderOwned<T> {
    message: String,
    options: Vec<T>,
    starting_cursor: Option<usize>,
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

    /// Entry point for select operations with owned values (doesn't require
    /// Clone)
    pub fn select_owned<T>(message: impl Into<String>, options: Vec<T>) -> SelectBuilderOwned<T> {
        SelectBuilderOwned { message: message.into(), options, starting_cursor: None }
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

    /// Prompt a question and get text input
    pub fn input(message: impl Into<String>) -> InputBuilder {
        InputBuilder { message: message.into(), allow_empty: false, default: None }
    }

    /// Multi-select prompt
    pub fn multi_select<T>(message: impl Into<String>, options: Vec<T>) -> MultiSelectBuilder<T> {
        MultiSelectBuilder { message: message.into(), options }
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
            let mut confirm = Confirm::with_theme(&theme).with_prompt(&self.message);

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

impl<T> SelectBuilderOwned<T> {
    /// Set starting cursor position
    pub fn with_starting_cursor(mut self, cursor: usize) -> Self {
        self.starting_cursor = Some(cursor);
        self
    }

    /// Execute select prompt with owned values
    pub fn prompt(self) -> Result<Option<T>>
    where
        T: std::fmt::Display,
    {
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
        Ok(idx_opt.and_then(|idx| self.options.into_iter().nth(idx)))
    }
}

/// Builder for input prompts
pub struct InputBuilder {
    message: String,
    allow_empty: bool,
    default: Option<String>,
}

impl InputBuilder {
    /// Allow empty input
    pub fn allow_empty(mut self, allow: bool) -> Self {
        self.allow_empty = allow;
        self
    }

    /// Set default value
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// Execute input prompt
    pub fn prompt(self) -> Result<Option<String>> {
        let theme = ForgeSelect::default_theme();
        let mut input = Input::with_theme(&theme)
            .with_prompt(&self.message)
            .allow_empty(self.allow_empty);

        if let Some(default) = self.default {
            input = input.default(default);
        }

        match input.interact_text() {
            Ok(value) => Ok(Some(value)),
            Err(_) => Ok(None), // User interrupted or error
        }
    }
}

/// Builder for multi-select prompts
pub struct MultiSelectBuilder<T> {
    message: String,
    options: Vec<T>,
}

impl<T> MultiSelectBuilder<T> {
    /// Execute multi-select prompt
    pub fn prompt(self) -> Result<Option<Vec<T>>>
    where
        T: std::fmt::Display + Clone,
    {
        if self.options.is_empty() {
            return Ok(None);
        }

        let theme = ForgeSelect::default_theme();
        let multi_select = MultiSelect::with_theme(&theme)
            .with_prompt(&self.message)
            .items(&self.options);

        let indices_opt = multi_select.interact_opt().map_err(anyhow::Error::from)?;

        Ok(indices_opt.map(|indices| {
            indices
                .into_iter()
                .filter_map(|idx| self.options.get(idx).cloned())
                .collect()
        }))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

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

    #[test]
    fn test_input_builder_creates() {
        let builder = ForgeSelect::input("Enter name:");
        assert_eq!(builder.message, "Enter name:");
        assert_eq!(builder.allow_empty, false);
    }

    #[test]
    fn test_multi_select_builder_creates() {
        let builder = ForgeSelect::multi_select("Select options:", vec!["a", "b", "c"]);
        assert_eq!(builder.message, "Select options:");
        assert_eq!(builder.options, vec!["a", "b", "c"]);
    }
}
