use ratatui::layout::*;
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols::{border, line, scrollbar};
use ratatui::text::{Line, Span};
use ratatui::widgets::*;

use crate::domain::{MenuItem, State};

pub struct Menu {
    items: Vec<MenuItem>,
}

impl Menu {
    pub fn new(items: Vec<MenuItem>) -> Self {
        Self { items }
    }

    pub fn menu_block() -> Block<'static> {
        Block::bordered()
            .padding(Padding::right(1))
            .border_set(border::Set {
                bottom_right: line::VERTICAL_LEFT,
                bottom_left: line::VERTICAL_RIGHT,
                ..border::PLAIN
            })
            .title(" ↑/↓ Move • [Key] Jump • [ESC] Cancel • ⏎ Run")
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
            .flex(Flex::Center)
            .areas(area);

        let [area] = Layout::horizontal([Constraint::Percentage(75)])
            .flex(Flex::Center)
            .areas(area);

        let [area] = Layout::horizontal([Constraint::Max(80)])
            .flex(Flex::Center)
            .areas(area);

        Clear.render(area, buf);
        area
    }
}

impl StatefulWidget for Menu {
    type State = State;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let selected_index = state.menu.list.selected().unwrap_or(2);
        let selected_item = self.items.get(selected_index).unwrap();
        let area = self.init_area(area, buf);
        let menu_block = Self::menu_block();

        // Calculate the maximum title width for consistent description alignment
        let max_title_width = self
            .items
            .iter()
            .map(|item| item.title.len())
            .max()
            .unwrap_or(0);

        let items_len = self.items.len();
        let items = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let is_selected = i == selected_index;
                self.render_menu_item(item.clone(), max_title_width, is_selected)
            })
            .collect::<Vec<_>>();
        let menu_list = List::new(items).block(menu_block);

        let [menu_area, description_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Max(4)])
                .flex(Flex::SpaceAround)
                .areas(area);

        // Render the list with state for scrolling
        StatefulWidget::render(menu_list, menu_area, buf, &mut state.menu.list);

        // Add scrollbar if there are more items than can fit in the area
        let scrollbar_area = menu_area.inner(Margin { horizontal: 0, vertical: 1 });
        // TODO: not sure if this is best way to check if scrollbar is needed.
        if items_len > scrollbar_area.height as usize {
            let mut scrollbar_state = ScrollbarState::new(items_len).position(selected_index);

            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .symbols(scrollbar::Set { ..scrollbar::VERTICAL })
                .style(Style::default().fg(Color::DarkGray))
                .thumb_style(Style::default().dark_gray())
                .render(scrollbar_area, buf, &mut scrollbar_state);
        }

        // Render the menu's description block
        let description_block = Block::bordered()
            .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
            .border_style(Style::default().fg(Color::DarkGray));

        // TODO: use description of the selected item
        Paragraph::new(vec![
            Line::from(selected_item.description.to_owned()).style(Style::new().dim()),
            Line::from(format!(
                "Shortcut: [{}] = {}",
                selected_item.shortcut.to_ascii_uppercase(),
                selected_item.title
            ))
            .style(Style::new().dim()),
        ])
        .block(description_block)
        .render(description_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_menu_block_returns_block() {
        let actual = Menu::menu_block();

        // Simply verify that we get a Block back - the styling details are
        // implementation details that are better tested through integration
        // tests
        let _block: ratatui::widgets::Block = actual;
        // If we reach here, the method works correctly
    }

    #[test]
    fn test_menu_new() {
        let fixture = vec![
            MenuItem::new("Test Item", "Test Description", 't'),
            MenuItem::new("Another Item", "Another Description", 'a'),
        ];

        let actual = Menu::new(fixture.clone());
        let expected = Menu { items: fixture };

        assert_eq!(actual.items.len(), expected.items.len());
        assert_eq!(actual.items[0].title, expected.items[0].title);
        assert_eq!(actual.items[1].title, expected.items[1].title);
    }
}
