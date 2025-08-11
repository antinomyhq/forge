use forge_app::{ConfirmationService, UserResponse};
use inquire::ui::{RenderConfig, Styled};
use inquire::{InquireError, Select};

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

    /// Handle inquire errors consistently - convert cancellation/interruption
    /// to Reject
    fn handle_inquire_error<T: UserResponse>(result: Result<T, InquireError>) -> T {
        match result {
            Ok(response) => response,
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                T::negative()
            }
            Err(_) => T::negative(),
        }
    }
}

impl ConfirmationService for ForgeConfirmation {
    fn request_user_confirmation<R: UserResponse>(&self, message: impl ToString) -> R {
        let choices = R::varients();
        let message = message.to_string();
        let select = Select::new(&message, choices)
            .with_render_config(Self::render_config())
            .with_help_message("Use arrow keys to navigate, Enter to select");

        Self::handle_inquire_error(select.prompt())
    }
}
