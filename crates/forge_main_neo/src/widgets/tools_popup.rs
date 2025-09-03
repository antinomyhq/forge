use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, StatefulWidget, Widget};

use crate::domain::State;

#[derive(Default)]
pub struct ToolsPopupWidget;

impl StatefulWidget for ToolsPopupWidget {
    type State = State;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let [area] = Layout::vertical([Constraint::Percentage(80)])
            .flex(Flex::Center)
            .areas(area);

        let [area] = Layout::horizontal([Constraint::Percentage(90)])
            .flex(Flex::Center)
            .areas(area);

        Clear.render(area, buf);

        let block = Block::bordered()
            .title_style(Style::default().bold())
            .border_style(Style::default().fg(Color::Green))
            .title_top(" AVAILABLE TOOLS ");

        let inner_area = block.inner(area);

        // Get tools from state
        let tools = &state.tools_popup.tools;

        if tools.is_empty() {
            let empty_message = List::new(vec![ListItem::new("No tools available")])
                .style(Style::default().fg(Color::Gray));

            Widget::render(empty_message, inner_area, buf);
        } else {
            // Create list items with just tool names
            let items: Vec<ListItem> = tools
                .iter()
                .map(|tool| {
                    let name = tool.name.to_string();
                    let line =
                        Line::from(Span::styled(name, Style::default().bold().fg(Color::Cyan)));
                    ListItem::new(line)
                })
                .collect();

            let tools_list = List::new(items);

            // Render the list without selection state
            Widget::render(tools_list, inner_area, buf);
        }

        // Render the block
        block.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use forge_api::ToolDefinition;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::*;

    #[test]
    fn test_tools_popup_widget_renders_without_panic() {
        let mut fixture_state = State::default();
        fixture_state.tools_popup.is_visible = true;
        fixture_state.tools_popup.tools =
            vec![ToolDefinition::new("test_tool").description("Test tool description")];

        let fixture_area = Rect::new(0, 0, 80, 24);
        let mut fixture_buffer = Buffer::empty(fixture_area);

        // Just test that rendering doesn't panic
        ToolsPopupWidget.render(fixture_area, &mut fixture_buffer, &mut fixture_state);

        // If we get here, the test passed
        assert!(true);
    }

    #[test]
    fn test_tools_popup_widget_handles_empty_tools() {
        let mut fixture_state = State::default();
        fixture_state.tools_popup.is_visible = true;
        fixture_state.tools_popup.tools = vec![];

        let fixture_area = Rect::new(0, 0, 80, 24);
        let mut fixture_buffer = Buffer::empty(fixture_area);

        // Just test that rendering doesn't panic
        ToolsPopupWidget.render(fixture_area, &mut fixture_buffer, &mut fixture_state);

        // If we get here, the test passed
        assert!(true);
    }
}
