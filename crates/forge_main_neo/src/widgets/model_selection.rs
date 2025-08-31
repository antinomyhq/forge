use edtui::{EditorTheme, EditorView};
use ratatui::layout::{Constraint, Flex, Layout, Margin};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols::{border, line};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, StatefulWidget, Widget,
};

use crate::domain::State;

#[derive(Default)]
pub struct ModelSelectionWidget;

impl StatefulWidget for ModelSelectionWidget {
    type State = State;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let [area] = Layout::vertical([Constraint::Percentage(75)])
            .flex(Flex::Center)
            .areas(area);

        let [area] = Layout::horizontal([Constraint::Percentage(80)])
            .flex(Flex::Center)
            .areas(area);

        Clear.render(area, buf);

        let [input_area, content_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(0)]).areas(area);

        let input_block = Block::bordered()
            .title_style(Style::default().bold())
            .border_set(border::Set {
                bottom_right: line::VERTICAL_LEFT,
                bottom_left: line::VERTICAL_RIGHT,
                ..border::PLAIN
            })
            .border_style(Style::default().fg(Color::Blue))
            .title_top(" SELECT MODEL ");

        EditorView::new(&mut state.model_selection.editor)
            .theme(
                EditorTheme::default()
                    .base(Style::reset())
                    .cursor_style(Style::default().fg(Color::Black).bg(Color::White))
                    .hide_status_line(),
            )
            .render(input_block.inner(input_area), buf);

        input_block.render(input_area, buf);

        // Show loading message if models are being loaded
        if state.model_selection.loading {
            let loading_text = Paragraph::new("Loading models...")
                .style(Style::default().fg(Color::Yellow))
                .block(
                    Block::bordered()
                        .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                        .border_style(Style::default().fg(Color::Blue)),
                );
            loading_text.render(content_area, buf);
            return;
        }

        // Get the current input text for filtering and store needed values
        // We need to collect all the data we need before any mutable borrows
        let models_data: Vec<_> = {
            let filtered_models = state.model_selection.filtered_models();
            filtered_models
                .iter()
                .map(|model| {
                    (
                        model.id.as_str().to_string(),
                        model.name.clone(),
                        model.context_length,
                        model.tools_supported.unwrap_or(false),
                    )
                })
                .collect()
        };
        let selected_index = state.model_selection.selected_index;
        let filtered_count = models_data.len();

        // Update the list state to reflect current selection
        state
            .model_selection
            .list_state
            .select(Some(selected_index));

        // Calculate the maximum width of filtered model IDs for consistent alignment
        let max_id_width = models_data
            .iter()
            .map(|(id, _, _, _)| id.len())
            .max()
            .unwrap_or(0);

        // Create list items with padded model IDs for aligned descriptions
        let items: Vec<ListItem> = models_data
            .iter()
            .enumerate()
            .map(|(i, (id, name_opt, context_length, tools_supported))| {
                let style = if i == selected_index {
                    Style::default().bg(Color::White).fg(Color::Black)
                } else {
                    Style::default()
                };

                let name = name_opt.as_ref().map(|s| s.as_str()).unwrap_or(id);

                // Pad the ID to the maximum width and add a separator
                let padded_id = format!("{id:<max_id_width$} ");

                let mut spans = vec![Span::styled(
                    padded_id,
                    Style::default().bold().fg(Color::Cyan),
                )];

                // Add model name if different from ID
                if name != id {
                    spans.push(Span::styled(name, Style::default().fg(Color::Green)));
                }

                // Add context length if available
                if let Some(context_length) = context_length {
                    spans.push(Span::styled(
                        format!(" [{}k]", context_length / 1000),
                        Style::default().fg(Color::Gray),
                    ));
                }

                // Add tool support indicator
                if *tools_supported {
                    spans.push(Span::styled(" 🛠️", Style::default()));
                }

                let line = Line::from(spans);
                ListItem::new(line).style(style)
            })
            .collect();

        let models_list = List::new(items)
            .block(
                Block::bordered()
                    .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .highlight_style(Style::default().bg(Color::White).fg(Color::Black));

        // Render the list with state for scrolling
        StatefulWidget::render(
            models_list,
            content_area,
            buf,
            &mut state.model_selection.list_state,
        );

        // Add scrollbar if there are more items than can fit in the area
        let scrollbar_area = content_area.inner(Margin { horizontal: 0, vertical: 1 });
        if filtered_count > scrollbar_area.height as usize {
            let mut scrollbar_state = ScrollbarState::new(filtered_count).position(selected_index);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::Blue))
                .thumb_style(Style::default().fg(Color::White));

            scrollbar.render(scrollbar_area, buf, &mut scrollbar_state);
        }
    }
}
