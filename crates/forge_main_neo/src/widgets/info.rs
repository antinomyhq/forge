use std::path::{Path, PathBuf};

use forge_api::ChatResponse;
use ratatui::layout::{Constraint, Flex, Layout, Margin};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Clear, List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
    Widget,
};

use crate::domain::State;

#[derive(Default)]
pub struct InfoWidget;

impl StatefulWidget for InfoWidget {
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

        let [area] = Layout::horizontal([Constraint::Percentage(70)])
            .flex(Flex::Center)
            .areas(area);

        Clear.render(area, buf);

        let info_block = Block::bordered()
            .title_style(Style::default().bold())
            .border_style(Style::default().fg(Color::Blue))
            .title_top(" SYSTEM INFO ");

        // Create info content
        let info_items = create_info_items(state);

        let items: Vec<ListItem> = info_items
            .iter()
            .map(|(key, value)| {
                if key.is_empty() {
                    // Section header
                    ListItem::new(Line::from(vec![Span::styled(
                        value,
                        Style::default().bold().fg(Color::Yellow),
                    )]))
                } else if value.is_empty() {
                    // Just key
                    ListItem::new(Line::from(vec![Span::styled(
                        key,
                        Style::default().fg(Color::Cyan),
                    )]))
                } else {
                    // Key-value pair
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{key}: "), Style::default().bold().fg(Color::Cyan)),
                        Span::styled(value, Style::default().fg(Color::White)),
                    ]))
                }
            })
            .collect();

        let info_list = List::new(items).block(info_block);

        Widget::render(info_list, area, buf);

        // Add scrollbar if needed
        let scrollbar_area = area.inner(Margin { horizontal: 0, vertical: 1 });
        if info_items.len() > scrollbar_area.height as usize {
            let mut scrollbar_state = ScrollbarState::new(info_items.len()).position(0);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::Blue))
                .thumb_style(Style::default().fg(Color::White));

            scrollbar.render(scrollbar_area, buf, &mut scrollbar_state);
        }
    }
}

struct UsageInfo {
    prompt_tokens: String,
    completion_tokens: String,
    total_tokens: String,
    cached_tokens: String,
    cost: Option<String>,
}

// TODO: Add the model info
fn extract_model_info(_state: &State) -> (String, String) {
    // For now, we don't have direct access to model information in ChatResponse
    // This would need to be tracked separately in the State or extracted from
    // conversation metadata
    (
        "Model information not available".to_string(),
        "Provider information not available".to_string(),
    )
}

fn extract_usage_info(state: &State) -> UsageInfo {
    // Look for the latest usage information from assistant messages
    for message in state.messages.iter().rev() {
        if let crate::domain::Message::Assistant(response) = message
            && let ChatResponse::Usage(usage) = response
        {
            return UsageInfo {
                prompt_tokens: usage.prompt_tokens.to_string(),
                completion_tokens: usage.completion_tokens.to_string(),
                total_tokens: usage.total_tokens.to_string(),
                cached_tokens: usage.cached_tokens.to_string(),
                cost: Some(usage.cost.map_or("N/A".to_string(), |c| format!("${c:.4}"))),
            };
        }
    }

    // Return default values if no usage information is found
    UsageInfo {
        prompt_tokens: "~0".to_string(),
        completion_tokens: "~0".to_string(),
        total_tokens: "~0".to_string(),
        cached_tokens: "~0".to_string(),
        cost: None,
    }
}

fn format_path_zsh_style(home: &Option<PathBuf>, path: &Path) -> String {
    if let Some(home) = home
        && let Ok(rel_path) = path.strip_prefix(home)
    {
        return format!("~/{}", rel_path.display());
    }
    path.display().to_string()
}

fn create_info_items(state: &State) -> Vec<(String, String)> {
    let mut items = Vec::new();

    // Model section
    items.push(("".to_string(), "MODEL".to_string()));

    // Extract model information from the latest assistant message if available
    let (current_model, provider_info) = extract_model_info(state);
    items.push(("Current".to_string(), current_model));
    items.push(("Provider (URL)".to_string(), provider_info));

    // Add empty line
    items.push(("".to_string(), "".to_string()));

    // Usage section
    items.push(("".to_string(), "USAGE".to_string()));

    // Extract usage information from the latest assistant message if available
    let usage_info = extract_usage_info(state);
    items.push(("Prompt".to_string(), usage_info.prompt_tokens));
    items.push(("Completion".to_string(), usage_info.completion_tokens));
    items.push(("Total".to_string(), usage_info.total_tokens));
    items.push(("Cached Tokens".to_string(), usage_info.cached_tokens));
    if let Some(cost) = usage_info.cost {
        items.push(("Cost".to_string(), cost));
    }

    // Add empty line
    items.push(("".to_string(), "".to_string()));

    // Environment section
    items.push(("".to_string(), "ENVIRONMENT".to_string()));
    let version = state.env.version();
    items.push(("Version".to_string(), version));

    if let Ok(cwd) = std::env::current_dir() {
        items.push((
            "Working Directory".to_string(),
            format_path_zsh_style(&state.env.home, &cwd),
        ));
    }

    items.push(("Shell".to_string(), state.env.shell.clone()));

    // Git branch info
    if let Some(branch) = get_git_branch() {
        items.push(("Git Branch".to_string(), branch));
    } else {
        items.push((
            "Git Branch".to_string(),
            "(not in a git repository)".to_string(),
        ));
    }

    // Add empty line
    items.push(("".to_string(), "".to_string()));

    // Paths section
    items.push(("".to_string(), "PATHS".to_string()));

    // Add forge-specific paths relative to home
    items.push((
        "Logs".to_string(),
        format_path_zsh_style(&state.env.home, &state.env.log_path()),
    ));
    items.push((
        "History".to_string(),
        format_path_zsh_style(&state.env.home, &state.env.history_path()),
    ));
    items.push((
        "Checkpoints".to_string(),
        format_path_zsh_style(&state.env.home, &state.env.snapshot_path()),
    ));

    // Add empty line
    items.push(("".to_string(), "".to_string()));

    items
}

fn get_git_branch() -> Option<String> {
    // First check if we're in a git repository
    let git_check = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .ok()?;

    if !git_check.status.success() || git_check.stdout.is_empty() {
        return None;
    }

    // If we are in a git repo, get the branch
    let output = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    }
}
