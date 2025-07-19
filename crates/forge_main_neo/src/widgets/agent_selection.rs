use convert_case::{Case, Casing};
use ratatui::widgets::StatefulWidget;

use crate::domain::{EditorStateExt, State};
use crate::widgets::popup::{GenericPopupWidget, PopupItem};

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

        // Extract the search text and available agents to avoid borrow conflicts
        let search_text = state.agent_selection.editor.get_text().to_lowercase();
        let available_agents = &state.agent_selection.available_agents;
        let selected_index = state.agent_selection.selected_index;

        // Filter agents manually to avoid borrowing conflicts
        let filtered_agents: Vec<_> = if search_text.is_empty() {
            available_agents.iter().collect()
        } else {
            available_agents
                .iter()
                .filter(|agent| {
                    // Search in agent ID
                    agent.id.as_str().to_lowercase().contains(&search_text) ||
                    // Search in agent title
                    agent.title.as_ref()
                        .map(|title| title.to_lowercase().contains(&search_text))
                        .unwrap_or(false)
                })
                .collect()
        };

        // Convert agents to popup items with owned strings for names
        let agent_names: Vec<String> = filtered_agents
            .iter()
            .map(|agent| agent.id.as_str().to_case(Case::UpperSnake))
            .collect();

        let items: Vec<PopupItem> = filtered_agents
            .iter()
            .zip(agent_names.iter())
            .map(|(agent, name)| PopupItem {
                name,
                description: agent.title.as_deref().unwrap_or("<Missing agent.title>"),
            })
            .collect();

        // Create the popup widget after getting all needed data
        GenericPopupWidget::new(
            "SEARCH AGENTS",
            items,
            &mut state.agent_selection.editor,
            selected_index,
        )
        .with_content_title(true)
        .render(area, buf);
    }
}
