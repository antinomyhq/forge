use forge_app::ConfirmationService;
use inquire::ui::{RenderConfig, Styled};
use inquire::Select;
use std::fmt::Display;

#[derive(Default, Debug, Clone)]
pub struct ForgeConfirmation;

impl ForgeConfirmation {
    /// Create a consistent render configuration for confirmation prompts
    fn render_config() -> RenderConfig<'static> {
        RenderConfig::default()
            .with_scroll_up_prefix(Styled::new("⇡"))
            .with_scroll_down_prefix(Styled::new("⇣"))
            .with_highlighted_option_prefix(Styled::new("➤"))
    }
}

impl ConfirmationService for ForgeConfirmation {
    fn request_user_confirmation<T: Display>(
        &self,
        message: impl ToString,
        choices: Vec<T>,
    ) -> Option<T> {
        let message = message.to_string();
        let select = Select::new(&message, choices)
            .with_render_config(Self::render_config())
            .with_help_message("Use arrow keys to navigate, Enter to select");

        select.prompt().ok()
    }
}
