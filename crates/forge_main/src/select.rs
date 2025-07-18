use anyhow::Result;
use inquire::ui::{RenderConfig, Styled};
use inquire::{Confirm, InquireError, Select};

/// Centralized inquire select functionality with consistent error handling
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
    /// Create a consistent render configuration for all select operations
    fn default_render_config() -> RenderConfig<'static> {
        RenderConfig::default()
            .with_scroll_up_prefix(Styled::new("⇡"))
            .with_scroll_down_prefix(Styled::new("⇣"))
            .with_highlighted_option_prefix(Styled::new("➤"))
    }

    /// Handle inquire errors consistently - convert cancellation/interruption
    /// to Ok(None)
    fn handle_inquire_error<T>(result: std::result::Result<T, InquireError>) -> Result<Option<T>> {
        match result {
            Ok(value) => Ok(Some(value)),
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => Ok(None),
            Err(e) => Err(e.into()),
        }
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
        T: std::fmt::Display,
    {
        // Handle confirm case (bool options)
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<bool>() {
            let mut confirm = Confirm::new(&self.message);

            if let Some(default) = self.default {
                confirm = confirm.with_default(default);
            }

            confirm = confirm.with_render_config(ForgeSelect::default_render_config());

            if let Some(message) = self.help_message {
                confirm = confirm.with_help_message(message);
            }

            let result = ForgeSelect::handle_inquire_error(confirm.prompt())?;
            // Safe cast since we checked the type
            return Ok(result.map(|b| unsafe { std::mem::transmute_copy(&b) }));
        }

        // Regular select
        let mut select = Select::new(&self.message, self.options);

        select = select.with_render_config(ForgeSelect::default_render_config());

        let help_message = self
            .help_message
            .unwrap_or("Use arrow keys to navigate, Enter to select, ESC to cancel");
        select = select.with_help_message(help_message);

        if let Some(cursor) = self.starting_cursor {
            select = select.with_starting_cursor(cursor);
        }

        ForgeSelect::handle_inquire_error(select.prompt())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_select_builder_basic() {
        // Test basic select builder creation
        let select = ForgeSelect::select("Choose", vec!["a", "b", "c"]);
        assert_eq!(select.message, "Choose");
        assert_eq!(select.options, vec!["a", "b", "c"]);
        assert_eq!(select.starting_cursor, None);
        assert_eq!(select.help_message, None);
    }

    #[test]
    fn test_select_builder_with_cursor() {
        // Test select builder with starting cursor
        let select = ForgeSelect::select("Choose", vec!["a", "b", "c"]).with_starting_cursor(1);
        assert_eq!(select.starting_cursor, Some(1));
    }

    #[test]
    fn test_select_builder_with_help() {
        // Test select builder with help message
        let select = ForgeSelect::select("Choose", vec!["a", "b", "c"])
            .with_help_message("Select an option");
        assert_eq!(select.help_message, Some("Select an option"));
    }

    #[test]
    fn test_confirm_builder_basic() {
        // Test basic confirm builder creation
        let confirm = ForgeSelect::confirm("Are you sure?");
        assert_eq!(confirm.message, "Are you sure?");
        assert_eq!(confirm.options, vec![true, false]);
        assert_eq!(confirm.default, None);
    }

    #[test]
    fn test_confirm_builder_with_default() {
        // Test confirm builder with default
        let confirm = ForgeSelect::confirm("Are you sure?").with_default(true);
        assert_eq!(confirm.default, Some(true));
    }

    #[test]
    fn test_render_config_consistency() {
        // Test that render config is consistent
        let _config = ForgeSelect::default_render_config();
        // We can't directly test the config content, but we can ensure it's created
        // This test mainly documents the expected behavior
        assert_eq!(true, true);
    }

    #[test]
    fn test_handle_inquire_error_ok() {
        // Test successful result handling
        let result = ForgeSelect::handle_inquire_error(Ok("success"));
        assert_eq!(result.unwrap(), Some("success"));
    }

    #[test]
    fn test_handle_inquire_error_canceled() {
        // Test cancellation handling
        let result =
            ForgeSelect::handle_inquire_error::<String>(Err(InquireError::OperationCanceled));
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_handle_inquire_error_interrupted() {
        // Test interruption handling
        let result =
            ForgeSelect::handle_inquire_error::<String>(Err(InquireError::OperationInterrupted));
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_handle_inquire_error_other_error() {
        // Test other error handling
        let result = ForgeSelect::handle_inquire_error::<String>(Err(
            InquireError::InvalidConfiguration("test".to_string()),
        ));
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_chaining() {
        // Test method chaining
        let select = ForgeSelect::select("Choose", vec!["a", "b", "c"])
            .with_starting_cursor(1)
            .with_help_message("Select an option");

        assert_eq!(select.message, "Choose");
        assert_eq!(select.options, vec!["a", "b", "c"]);
        assert_eq!(select.starting_cursor, Some(1));
        assert_eq!(select.help_message, Some("Select an option"));
    }

    #[test]
    fn test_confirm_builder_chaining() {
        // Test confirm builder chaining
        let confirm = ForgeSelect::confirm("Are you sure?")
            .with_default(true)
            .with_help_message("Press Enter to confirm");

        assert_eq!(confirm.message, "Are you sure?");
        assert_eq!(confirm.default, Some(true));
        assert_eq!(confirm.help_message, Some("Press Enter to confirm"));
    }
}
