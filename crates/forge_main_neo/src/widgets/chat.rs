use edtui::{EditorTheme, EditorView};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::symbols::{border, line};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, StatefulWidget, Widget};
use strum::IntoEnumIterator;

use crate::domain::{EditorStateExt, SlashCommand, State};
use crate::widgets::menu::MenuWidget;
use crate::widgets::message_list::MessageList;
use crate::widgets::status_bar::StatusBar;
use crate::widgets::welcome::WelcomeWidget;

/// Chat widget that handles the chat interface with editor and message list
#[derive(Clone, Default)]
pub struct ChatWidget;

impl StatefulWidget for ChatWidget {
    type State = State;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut State,
    ) where
        Self: Sized,
    {
        // Update menu visibility based on current state
        state.update_menu_visibility();

        // Create chat layout with messages area at top and user input area at bottom
        let chat_layout = Layout::new(
            Direction::Vertical,
            [Constraint::Fill(1), Constraint::Max(5), Constraint::Max(1)],
        );

        let [messages_area, user_area, status_area] = chat_layout.areas(area);

        // Messages area block
        let message_block = if state.menu_visible {
            Block::bordered()
                .borders(ratatui::widgets::Borders::ALL - ratatui::widgets::Borders::BOTTOM)
        } else {
            Block::bordered()
        };

        // Render welcome widget if no messages, otherwise render message list
        if state.messages.is_empty() {
            WelcomeWidget.render(message_block.inner(messages_area), buf, state);
        } else {
            MessageList.render(message_block.inner(messages_area), buf, state);
        }

        // Render menu when visible
        if state.menu_visible {
            if state.slash_menu_visible() {
                // Get the current search term (everything after "/")
                let text = state.editor.get_text();
                let search_term = text.strip_prefix('/').unwrap_or("");

                // Get filtered commands using fuzzy search
                let filtered_commands = crate::domain::SlashCommand::fuzzy_filter(search_term);
                MenuWidget::new(filtered_commands).render(messages_area, buf, state);
            } else {
                // Show all commands when in normal mode
                MenuWidget::new(SlashCommand::iter().collect()).render(messages_area, buf, state);
            }
        }

        let title = Span::styled(
            " Input ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
        // User input area block
        let user_block = Block::bordered()
            .padding(Padding::new(0, 0, 0, 1))
            .border_style(Style::default().dark_gray())
            .border_set(if state.menu_visible {
                border::Set {
                    top_left: line::VERTICAL_RIGHT,
                    top_right: line::VERTICAL_LEFT,
                    ..border::PLAIN
                }
            } else {
                border::PLAIN
            })
            .title(title);

        EditorView::new(&mut state.editor)
            .theme(
                EditorTheme::default()
                    .base(Style::reset())
                    .cursor_style(Style::default().fg(Color::Black).bg(Color::White))
                    .hide_status_line(),
            )
            .wrap(true)
            .render(user_block.inner(user_area), buf);

        // Render blocks
        message_block.render(messages_area, buf);

        // Render Status Bar
        let status_bar = StatusBar::new("FORGE", state.editor.mode.name(), state.workspace.clone());

        user_block.render(user_area, buf);
        ratatui::widgets::Paragraph::new(Line::from(status_bar)).render(status_area, buf);
    }
}
