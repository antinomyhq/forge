use forge_app::{ConfirmationService, UserResponse};
use inquire::ui::{RenderConfig, Styled};
use inquire::{InquireError, Select};
use strum::IntoEnumIterator;

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
    fn handle_inquire_error(
        result: std::result::Result<UserResponse, InquireError>,
    ) -> UserResponse {
        match result {
            Ok(response) => response,
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                UserResponse::Reject
            }
            Err(_) => UserResponse::Reject,
        }
    }
}

impl ConfirmationService for ForgeConfirmation {
    fn request_user_confirmation(&self) -> UserResponse {
        let choices: Vec<UserResponse> = UserResponse::iter().collect();

        let select = Select::new(
            "This operation requires confirmation. How would you like to proceed?",
            choices,
        )
        .with_render_config(Self::render_config())
        .with_help_message("Use arrow keys to navigate, Enter to select");

        Self::handle_inquire_error(select.prompt())
    }
}
