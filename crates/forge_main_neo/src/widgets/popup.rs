use edtui::{EditorState, EditorTheme, EditorView};
use ratatui::layout::{Constraint, Flex, Layout, Margin};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols::{border, line};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState,
    StatefulWidget, Widget,
};

/// Generic popup widget that can display a searchable list of items
pub struct GenericPopupWidget<'a> {
    pub title: &'a str,
    pub items: Vec<PopupItem<'a>>,
    pub editor: &'a mut EditorState,
    pub selected_index: usize,
    pub show_title_on_content: bool,
}

/// Represents an item that can be displayed in the popup
pub struct PopupItem<'a> {
    pub name: &'a str,
    pub description: &'a str,
}

impl<'a> GenericPopupWidget<'a> {
    pub fn new(
        title: &'a str,
        items: Vec<PopupItem<'a>>,
        editor: &'a mut EditorState,
        selected_index: usize,
    ) -> Self {
        Self {
            title,
            items,
            editor,
            selected_index,
            show_title_on_content: false,
        }
    }

    pub fn with_content_title(mut self, show: bool) -> Self {
        self.show_title_on_content = show;
        self
    }

    pub fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let [area] = Layout::vertical([Constraint::Percentage(75)])
            .flex(Flex::Center)
            .areas(area);

        let [area] = Layout::horizontal([Constraint::Percentage(80)])
            .flex(Flex::Center)
            .areas(area);

        Clear.render(area, buf);

        let [input_area, content_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(0)]).areas(area);

        // Render search input
        let input_block = Block::bordered()
            .title_style(Style::default().bold())
            .border_set(border::Set {
                bottom_right: line::VERTICAL_LEFT,
                bottom_left: line::VERTICAL_RIGHT,
                ..border::PLAIN
            })
            .border_style(Style::default().fg(Color::Blue))
            .title_top(format!(" {} ", self.title));

        EditorView::new(self.editor)
            .theme(
                EditorTheme::default()
                    .base(Style::reset())
                    .cursor_style(Style::default().fg(Color::Black).bg(Color::White))
                    .hide_status_line(),
            )
            .render(input_block.inner(input_area), buf);

        input_block.render(input_area, buf);

        // Calculate the maximum width of item names for consistent alignment
        let max_name_width = self
            .items
            .iter()
            .map(|item| item.name.len())
            .max()
            .unwrap_or(0);

        // Create list items with names and descriptions
        let list_items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == self.selected_index {
                    Style::default().bg(Color::White).fg(Color::Black)
                } else {
                    Style::default()
                };

                // Pad the name to the maximum width and add a separator
                let padded_name = format!("{:<max_name_width$} ", item.name);

                let line = Line::from(vec![
                    Span::styled(padded_name, Style::default().bold().fg(Color::Cyan)),
                    Span::styled(item.description, Style::default().fg(Color::Green)),
                ]);

                ListItem::new(line).style(style)
            })
            .collect();

        let content_block = if self.show_title_on_content {
            Block::bordered()
                .title_style(Style::default().bold())
                .border_style(Style::default().fg(Color::Blue))
                .title_top(format!(" {} ", self.title))
        } else {
            Block::bordered()
                .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                .border_style(Style::default().fg(Color::Blue))
        };

        let items_list = List::new(list_items)
            .block(content_block)
            .highlight_style(Style::default().bg(Color::White).fg(Color::Black));

        // Render the list
        Widget::render(items_list, content_area, buf);

        // Add scrollbar if there are more items than can fit in the area
        let scrollbar_area = content_area.inner(Margin { horizontal: 0, vertical: 1 });
        if self.items.len() > scrollbar_area.height as usize {
            let mut scrollbar_state =
                ScrollbarState::new(self.items.len()).position(self.selected_index);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::Blue))
                .thumb_style(Style::default().fg(Color::White));

            scrollbar.render(scrollbar_area, buf, &mut scrollbar_state);
        }
    }
}
