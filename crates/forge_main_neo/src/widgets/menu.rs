use ratatui::layout::*;
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols::{border, line};
use ratatui::text::{Line, Span};
use ratatui::widgets::*;

use crate::domain::{MenuItem, State};

pub struct MenuWidget<T> {
    items: Vec<T>,
}

impl<T: Into<MenuItem>> MenuWidget<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { items }
    }

    pub fn menu_block() -> Block<'static> {
        Block::bordered()
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
            .padding(Padding::symmetric(1, 1))
            .border_set(border::Set {
                top_right: line::VERTICAL_LEFT,
                top_left: line::VERTICAL_RIGHT,
                ..border::PLAIN
            })
            .title_bottom(" ↑/↓ Move • ⏎ Run • [ESC] Cancel ")
            .title_style(Style::default().fg(Color::DarkGray))
            .border_style(Style::default().fg(Color::DarkGray))
    }

    fn render_menu_item(
        &self,
        item: MenuItem,
        max_title_width: usize,
        is_selected: bool,
    ) -> ListItem<'static> {
        let max_title_width = max_title_width + 1;
        let char = item.shortcut;
        let title = format!("{:<max_title_width$}", item.title);
        let description = item.description;

        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled(
            format!("[{}] ", char.to_ascii_uppercase()),
            Style::new().bold().yellow(),
        ));

        let mut style = Style::default().bold();

        if is_selected {
            style = style.fg(Color::White)
        }

        // Find the first occurrence of the trigger letter in the title (case
        // insensitive)
        if let Some(pos) = title
            .to_lowercase()
            .as_str()
            .find(char.to_ascii_lowercase())
        {
            // Add text before the trigger letter
            if pos > 0 {
                spans.push(Span::styled(title[..pos].to_owned(), style));
            }

            // Add the highlighted trigger letter
            spans.push(Span::styled(
                title[pos..pos + 1].to_owned(),
                style.underlined(),
            ));

            // Add text after the trigger letter
            if pos + 1 < title.len() {
                spans.push(Span::styled(title[pos + 1..].to_owned(), style));
            }
        } else {
            // Fallback if trigger letter not found in title
            spans.push(Span::styled(title, style))
        }

        // Add description with calculated padding
        spans.push(Span::styled(description.to_string(), style));

        let mut style = Style::new();
        if is_selected {
            style = style.bg(Color::Cyan);
        }
        ListItem::new(Line::from(spans).style(style))
    }

    fn init_area(&self, area: Rect, buf: &mut ratatui::prelude::Buffer) -> Rect {
        let [area] = Layout::vertical([Constraint::Max(15)])
            .flex(Flex::End)
            .areas(area);

        Clear.render(area, buf);
        area
    }
}

impl<T> StatefulWidget for MenuWidget<T>
where
    T: Into<MenuItem> + Clone,
{
    type State = State;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let selected_index = state.menu.list.selected().unwrap_or(0);
        let selected_menu_item: Option<MenuItem> = self
            .items
            .get(selected_index)
            .map(|item| item.clone().into());
        let area = self.init_area(area, buf);
        let menu_block = Self::menu_block();
        let items_len = self.items.len();

        // Calculate the maximum title width for consistent description alignment
        let menu_items = self
            .items
            .iter()
            .map(|item| Into::<MenuItem>::into(item.to_owned()))
            .collect::<Vec<_>>();
        let max_title_width = menu_items
            .iter()
            .map(|item| item.title.len())
            .max()
            .unwrap_or(0);

        let list_items = menu_items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let is_selected = i == selected_index;
                self.render_menu_item(item, max_title_width, is_selected)
            })
            .collect::<Vec<_>>();
        let menu_list = List::new(list_items).block(menu_block);

        let [description_area, menu_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Fill(4)]).areas(area);

        // Render the list with state for scrolling
        StatefulWidget::render(menu_list, menu_area, buf, &mut state.menu.list);

        // Add scrollbar if there are more items than can fit in the area
        let scrollbar_area = menu_area.inner(Margin { horizontal: 1, vertical: 1 });
        // TODO: not sure if this is best way to check if scrollbar is needed.
        if items_len > scrollbar_area.height as usize {
            let mut scrollbar_state = ScrollbarState::new(items_len).position(selected_index);

            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::DarkGray))
                .render(scrollbar_area, buf, &mut scrollbar_state);
        }

        if let Some(selected_item) = selected_menu_item {
            // Render the menu's description block
            let description_block = Block::new()
                .borders(Borders::LEFT | Borders::RIGHT | Borders::TOP)
                .border_set(border::Set {
                    bottom_right: line::VERTICAL_LEFT,
                    bottom_left: line::VERTICAL_RIGHT,
                    ..border::PLAIN
                })
                .padding(Padding::symmetric(1, 0))
                .border_style(Style::default().fg(Color::DarkGray));

            // TODO: use description of the selected item
            Paragraph::new(vec![
                Line::from(selected_item.description.to_owned()).style(Style::new().dim()),
                Line::from(format!(
                    "Shortcut: [⇧ + {}] = {}",
                    selected_item.shortcut.to_ascii_uppercase(),
                    selected_item.title
                ))
                .style(Style::new().dim()),
            ])
            .block(description_block)
            .render(description_area, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_menu_new() {
        let fixture = vec![
            MenuItem::new("Test Item", "Test Description", 't'),
            MenuItem::new("Another Item", "Another Description", 'a'),
        ];

        let actual = MenuWidget::new(fixture.clone());
        let expected = MenuWidget { items: fixture };

        assert_eq!(actual.items.len(), expected.items.len());
        assert_eq!(actual.items[0].title, expected.items[0].title);
        assert_eq!(actual.items[1].title, expected.items[1].title);
    }
}
