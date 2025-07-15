use edtui::{EditorMode, EditorState};
use forge_api::Model;
use ratatui::widgets::ListState;

use crate::domain::editor_helpers::EditorStateExt;

#[derive(Clone)]
pub struct ModelSelectionState {
    pub is_visible: bool,
    pub editor: EditorState,
    pub selected_index: usize,
    pub list_state: ListState,
    pub models: Vec<Model>,
    pub loading: bool,
}

impl Default for ModelSelectionState {
    fn default() -> Self {
        let mut editor = EditorState::default();
        editor.mode = EditorMode::Insert;

        Self {
            is_visible: false,
            editor,
            selected_index: 0,
            list_state: ListState::default(),
            models: Vec::new(),
            loading: false,
        }
    }
}

impl ModelSelectionState {
    /// Get the currently selected model
    pub fn selected_model(&self) -> Option<&Model> {
        let input_text = self.editor.get_text().to_lowercase();

        // Filter models that match the input text
        let filtered_models: Vec<&Model> = self
            .models
            .iter()
            .filter(|model| {
                model.id.as_str().to_lowercase().contains(&input_text)
                    || model
                        .name
                        .as_ref()
                        .is_some_and(|name| name.to_lowercase().contains(&input_text))
            })
            .collect();

        filtered_models.get(self.selected_index).copied()
    }

    /// Get all models that match the current input filter
    pub fn filtered_models(&self) -> Vec<&Model> {
        let input_text = self.editor.get_text().to_lowercase();

        self.models
            .iter()
            .filter(|model| {
                model.id.as_str().to_lowercase().contains(&input_text)
                    || model
                        .name
                        .as_ref()
                        .is_some_and(|name| name.to_lowercase().contains(&input_text))
            })
            .collect()
    }

    /// Set the models list
    pub fn set_models(&mut self, models: Vec<Model>) {
        self.models = models;
        self.selected_index = 0;
        self.loading = false;
    }

    /// Show the modal and start loading models
    pub fn show(&mut self) {
        self.is_visible = true;
        self.loading = true;
        self.editor.clear();
        self.selected_index = 0;
    }

    /// Hide the modal
    pub fn hide(&mut self) {
        self.is_visible = false;
        self.loading = false;
        self.editor.clear();
        self.selected_index = 0;
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        let filtered_count = self.filtered_models().len();
        if filtered_count > 0 {
            self.selected_index = if self.selected_index == 0 {
                filtered_count - 1
            } else {
                self.selected_index - 1
            };
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        let filtered_count = self.filtered_models().len();
        if filtered_count > 0 {
            self.selected_index = (self.selected_index + 1) % filtered_count;
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_api::ModelId;
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_test_model(id: &str, name: Option<&str>) -> Model {
        Model {
            id: ModelId::new(id),
            name: name.map(|s| s.to_string()),
            description: None,
            context_length: None,
            tools_supported: None,
            supports_parallel_tool_calls: None,
            supports_reasoning: None,
        }
    }

    #[test]
    fn test_model_selection_state_default() {
        let fixture = ModelSelectionState::default();
        let actual = fixture.is_visible;
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_model_selection_state_show() {
        let mut fixture = ModelSelectionState::default();
        fixture.show();
        let actual = fixture.is_visible;
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_model_selection_state_hide() {
        let mut fixture = ModelSelectionState::default();
        fixture.show();
        fixture.hide();
        let actual = fixture.is_visible;
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_filtered_models_empty_input() {
        let mut fixture = ModelSelectionState::default();
        let models = vec![
            create_test_model("gpt-4", Some("GPT-4")),
            create_test_model("claude-3", Some("Claude 3")),
        ];
        fixture.set_models(models);

        let actual = fixture.filtered_models().len();
        let expected = 2;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_filtered_models_with_input() {
        let mut fixture = ModelSelectionState::default();
        let models = vec![
            create_test_model("gpt-4", Some("GPT-4")),
            create_test_model("claude-3", Some("Claude 3")),
        ];
        fixture.set_models(models);
        fixture.editor.set_text_insert_mode("gpt".to_string());

        let actual = fixture.filtered_models().len();
        let expected = 1;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_selected_model() {
        let mut fixture = ModelSelectionState::default();
        let models = vec![
            create_test_model("gpt-4", Some("GPT-4")),
            create_test_model("claude-3", Some("Claude 3")),
        ];
        fixture.set_models(models);

        let actual = fixture.selected_model().unwrap().id.as_str();
        let expected = "gpt-4";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_select_next() {
        let mut fixture = ModelSelectionState::default();
        let models = vec![
            create_test_model("gpt-4", Some("GPT-4")),
            create_test_model("claude-3", Some("Claude 3")),
        ];
        fixture.set_models(models);

        fixture.select_next();
        let actual = fixture.selected_index;
        let expected = 1;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_select_previous() {
        let mut fixture = ModelSelectionState::default();
        let models = vec![
            create_test_model("gpt-4", Some("GPT-4")),
            create_test_model("claude-3", Some("Claude 3")),
        ];
        fixture.set_models(models);
        fixture.selected_index = 1;

        fixture.select_previous();
        let actual = fixture.selected_index;
        let expected = 0;
        assert_eq!(actual, expected);
    }
}
