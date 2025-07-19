use ratatui::widgets::StatefulWidget;
use strum::IntoEnumIterator;

use crate::domain::{EditorStateExt, SlashCommand, State};
use crate::widgets::popup::{GenericPopupWidget, PopupItem};

#[derive(Default)]
pub struct SpotlightWidget;

impl SpotlightWidget {}

impl StatefulWidget for SpotlightWidget {
    type State = State;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        // Extract data to avoid borrow conflicts
        let input_text = state.spotlight.editor.get_text().to_lowercase();
        let selected_index = state.spotlight.selected_index;

        // Update the list state to reflect current selection
        state.spotlight.list_state.select(Some(selected_index));

        // Filter commands manually to avoid borrowing conflicts
        let filtered_commands: Vec<_> = SlashCommand::iter()
            .filter(|cmd| cmd.to_string().to_lowercase().starts_with(&input_text))
            .collect();

        // Convert commands to popup items with owned strings for names
        let command_names: Vec<String> = filtered_commands
            .iter()
            .map(|cmd| cmd.to_string())
            .collect();

        let items: Vec<PopupItem> = filtered_commands
            .iter()
            .zip(command_names.iter())
            .map(|(cmd, name)| PopupItem { name, description: cmd.description() })
            .collect();

        // Create the popup widget after getting all needed data
        GenericPopupWidget::new(
            "SPOTLIGHT",
            items,
            &mut state.spotlight.editor,
            selected_index,
        )
        .render(area, buf);
    }
}
