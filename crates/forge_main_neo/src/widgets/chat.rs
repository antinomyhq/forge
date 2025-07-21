use edtui::{EditorMode, EditorTheme, EditorView};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols::{border, line};
use ratatui::widgets::{Block, Padding, StatefulWidget, Widget};

use crate::domain::{MenuItems, State};
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
        let is_normal_mode = state.editor.mode == EditorMode::Normal;

        // Create chat layout with messages area at top and user input area at bottom
        let chat_layout = Layout::new(
            Direction::Vertical,
            [Constraint::Fill(0), Constraint::Max(5)],
        );
        let [messages_area, user_area] = chat_layout.areas(area);

        // Messages area block (now at top)
        let message_block = Block::new();

        // Render welcome widget if no messages, otherwise render message list
        if state.messages.is_empty() {
            WelcomeWidget.render(message_block.inner(messages_area), buf, state);
        } else {
            MessageList.render(message_block.inner(messages_area), buf, state);
        }

        if is_normal_mode {
            MenuWidget::new(MenuItems::new().to_vec()).render(messages_area, buf, state);
        }

        // Render spotlight when visible
        if state.spotlight.is_visible {
            use crate::widgets::spotlight::SpotlightWidget;
            SpotlightWidget.render(messages_area, buf, state);
        }

        // Render slash command menu when visible
        if state.slash_menu_visible {
            // Get the current search term (everything after "/")
            let text = state.editor.get_text();
            let search_term = text.strip_prefix('/').unwrap_or("");

            // Get filtered commands using fuzzy search
            let filtered_commands = crate::domain::SlashCommand::fuzzy_filter(search_term);
            MenuWidget::new(filtered_commands).render(messages_area, buf, state);
        }

        // User input area block with status bar (now at bottom)
        let user_block = Block::bordered()
            .padding(Padding::new(0, 0, 0, 1))
            .border_style(Style::default().dark_gray())
            .border_set(if is_normal_mode {
                border::Set {
                    top_left: line::VERTICAL_RIGHT,
                    top_right: line::VERTICAL_LEFT,
                    ..border::PLAIN
                }
            } else {
                border::PLAIN
            })
            .title_bottom(StatusBar::new(
                "FORGE",
                state.editor.mode.name(),
                state.workspace.clone(),
            ));

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
        user_block.render(user_area, buf);
    }
}
