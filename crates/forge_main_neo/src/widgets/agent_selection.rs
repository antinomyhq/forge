use convert_case::{Case, Casing};
use edtui::{EditorTheme, EditorView};
use ratatui::layout::{Constraint, Flex, Layout, Margin};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols::{border, line};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Clear, List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
    Widget,
};

use crate::domain::State;

#[derive(Default)]
pub struct AgentSelectionWidget;

impl StatefulWidget for AgentSelectionWidget {
    type State = State;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        if !state.agent_selection.is_visible {
            return;
        }

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
            .title_top(" SEARCH AGENTS ");

        EditorView::new(&mut state.agent_selection.editor)
            .theme(
                EditorTheme::default()
                    .base(Style::reset())
                    .cursor_style(Style::default().fg(Color::Black).bg(Color::White))
                    .hide_status_line(),
            )
            .render(input_block.inner(input_area), buf);

        input_block.render(input_area, buf);

        // Get filtered agents based on search input
        let filtered_agents = state.agent_selection.filtered_agents();

        // Calculate the maximum width of agent names for consistent alignment
        let max_name_width = filtered_agents
            .iter()
            .map(|agent| agent.id.as_str().len())
            .max()
            .unwrap_or(0);

        // Create list items with agent names and descriptions
        let items: Vec<ListItem> = filtered_agents
            .iter()
            .enumerate()
            .map(|(i, agent)| {
                let style = if i == state.agent_selection.selected_index {
                    Style::default().bg(Color::White).fg(Color::Black)
                } else {
                    Style::default()
                };

                let name = agent.id.as_str().to_case(Case::UpperSnake);
                let desc = agent.title.as_deref().unwrap_or("<Missing agent.title>");

                // Pad the name to the maximum width and add a separator
                let padded_name = format!("{name:<max_name_width$} ");

                let line = Line::from(vec![
                    Span::styled(padded_name, Style::default().bold().fg(Color::Cyan)),
                    Span::styled(desc, Style::default().fg(Color::Green)),
                ]);

                ListItem::new(line).style(style)
            })
            .collect();

        let agents_list = List::new(items)
            .block(
                Block::bordered()
                    .title_style(Style::default().bold())
                    .border_style(Style::default().fg(Color::Blue))
                    .title_top(" SELECT AGENT "),
            )
            .highlight_style(Style::default().bg(Color::White).fg(Color::Black));

        // Render the list
        Widget::render(agents_list, content_area, buf);

        // Add scrollbar if there are more items than can fit in the area
        let scrollbar_area = content_area.inner(Margin { horizontal: 0, vertical: 1 });
        if filtered_agents.len() > scrollbar_area.height as usize {
            let mut scrollbar_state = ScrollbarState::new(filtered_agents.len())
                .position(state.agent_selection.selected_index);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::Blue))
                .thumb_style(Style::default().fg(Color::White));

            scrollbar.render(scrollbar_area, buf, &mut scrollbar_state);
        }
    }
}
