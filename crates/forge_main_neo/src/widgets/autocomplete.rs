use edtui::{EditorTheme, EditorView};
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols::{border, line};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, StatefulWidget, Widget};

use crate::domain::State;

#[derive(Default)]
pub struct AutocompleteWidget;

impl AutocompleteWidget {}

impl StatefulWidget for AutocompleteWidget {
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

        let (_is_autocomplete, autocomplete_state) = match &mut state.layover_state {
            crate::domain::LayoverState::Autocomplete(autocomplete_state) => {
                (true, Some(autocomplete_state))
            }
            _ => (false, None),
        };

        let input_block = Block::bordered()
            .title_style(Style::default().bold())
            .border_set(border::Set {
                bottom_right: line::VERTICAL_LEFT,
                bottom_left: line::VERTICAL_RIGHT,
                ..border::PLAIN
            })
            .border_style(Style::default().fg(Color::Blue))
            .title_top(" FILE SEARCH ");

        // Calculate the inner area before rendering the block
        let editor_area = input_block.inner(input_area);

        // Now render the block
        input_block.render(input_area, buf);

        if let Some(autocomplete_state) = autocomplete_state {
            EditorView::new(&mut autocomplete_state.editor)
                .theme(
                    EditorTheme::default()
                        .base(Style::reset())
                        .cursor_style(Style::default().fg(Color::Black).bg(Color::White))
                        .hide_status_line(),
                )
                .render(editor_area, buf);

            let selected_index = autocomplete_state.list_state.selected().unwrap_or(0);
            let max_name_width = autocomplete_state
                .suggestions
                .iter()
                .map(|s| s.len())
                .max()
                .unwrap_or(0);
            let items: Vec<ListItem> = autocomplete_state
                .suggestions
                .iter()
                .enumerate()
                .map(|(i, suggestion)| {
                    let style = if i == selected_index {
                        Style::default().bg(Color::White).fg(Color::Black)
                    } else {
                        Style::default()
                    };
                    let padded_name = format!("{suggestion:<max_name_width$} ");
                    // For autocomplete, no description, just the name
                    let line = Line::from(vec![Span::styled(
                        padded_name,
                        Style::default().bold().fg(Color::Cyan),
                    )]);
                    ListItem::new(line).style(style)
                })
                .collect();
            let autocomplete_list = List::new(items)
                .block(
                    Block::bordered()
                        .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                        .border_style(Style::default().fg(Color::Blue)),
                )
                .highlight_style(Style::default().bg(Color::White).fg(Color::Black));
            StatefulWidget::render(
                autocomplete_list,
                content_area,
                buf,
                &mut autocomplete_state.list_state,
            );
        }
    }
}
