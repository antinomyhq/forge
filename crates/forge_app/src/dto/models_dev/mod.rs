use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Response structure from models.dev API
///
/// The API returns a map where keys are provider IDs and values contain
/// provider metadata and their models.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelsDevResponse(pub HashMap<String, ProviderData>);

/// Provider data from models.dev
///
/// Contains provider metadata and a map of models offered by the provider.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProviderData {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub doc: Option<String>,
    pub models: HashMap<String, ModelData>,
}

/// Model data from models.dev
///
/// Contains comprehensive metadata about a specific model including
/// capabilities, pricing, and technical specifications.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelData {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub attachment: Option<bool>,
    #[serde(default)]
    pub reasoning: Option<bool>,
    #[serde(default)]
    pub tool_call: Option<bool>,
    #[serde(default)]
    pub temperature: Option<bool>,
    #[serde(default)]
    pub knowledge: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub modalities: Option<Modalities>,
    #[serde(default)]
    pub open_weights: Option<bool>,
    #[serde(default)]
    pub cost: Option<Cost>,
    #[serde(default)]
    pub limit: Option<Limit>,
}

/// Input and output modalities supported by a model
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Modalities {
    #[serde(default)]
    pub input: Vec<String>,
    #[serde(default)]
    pub output: Vec<String>,
}

/// Pricing information for model usage
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Cost {
    #[serde(default)]
    pub input: Option<f32>,
    #[serde(default)]
    pub output: Option<f32>,
    #[serde(default)]
    pub cache_read: Option<f32>,
}

/// Context and output limits for a model
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Limit {
    #[serde(default)]
    pub context: Option<u64>,
    #[serde(default)]
    pub output: Option<u64>,
}

impl From<ModelData> for forge_domain::Model {
    fn from(value: ModelData) -> Self {
        // Parse input modalities from models.dev format
        let input_modalities = value
            .modalities
            .as_ref()
            .map(|m| {
                m.input
                    .iter()
                    .filter_map(|s| s.parse::<forge_domain::InputModality>().ok())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| vec![forge_domain::InputModality::Text]);

        forge_domain::Model {
            id: value.id.into(),
            name: value.name,
            description: None,
            context_length: value.limit.and_then(|l| l.context),
            tools_supported: value.tool_call,
            supports_parallel_tool_calls: None,
            supports_reasoning: value.reasoning,
            input_modalities,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::InputModality;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_model_data_conversion() {
        let fixture = ModelData {
            id: "test-model".to_string(),
            name: Some("Test Model".to_string()),
            family: Some("test".to_string()),
            attachment: Some(false),
            reasoning: Some(true),
            tool_call: Some(true),
            temperature: Some(true),
            knowledge: Some("2024-10".to_string()),
            release_date: Some("2024-10-01".to_string()),
            last_updated: Some("2024-10-15".to_string()),
            modalities: Some(Modalities {
                input: vec!["text".to_string(), "image".to_string()],
                output: vec!["text".to_string()],
            }),
            open_weights: Some(true),
            cost: Some(Cost {
                input: Some(0.001),
                output: Some(0.002),
                cache_read: Some(0.0001),
            }),
            limit: Some(Limit {
                context: Some(128000),
                output: Some(4096),
            }),
        };

        let actual: forge_domain::Model = fixture.into();

        let expected = forge_domain::Model {
            id: "test-model".to_string().into(),
            name: Some("Test Model".to_string()),
            description: None,
            context_length: Some(128000),
            tools_supported: Some(true),
            supports_parallel_tool_calls: None,
            supports_reasoning: Some(true),
            input_modalities: vec![InputModality::Text, InputModality::Image],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_model_data_conversion_minimal() {
        let fixture = ModelData {
            id: "minimal-model".to_string(),
            name: None,
            family: None,
            attachment: None,
            reasoning: None,
            tool_call: None,
            temperature: None,
            knowledge: None,
            release_date: None,
            last_updated: None,
            modalities: None,
            open_weights: None,
            cost: None,
            limit: None,
        };

        let actual: forge_domain::Model = fixture.into();

        let expected = forge_domain::Model {
            id: "minimal-model".to_string().into(),
            name: None,
            description: None,
            context_length: None,
            tools_supported: None,
            supports_parallel_tool_calls: None,
            supports_reasoning: None,
            input_modalities: vec![InputModality::Text],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_modalities_parsing() {
        let fixture = ModelData {
            id: "multimodal".to_string(),
            name: None,
            family: None,
            attachment: None,
            reasoning: None,
            tool_call: None,
            temperature: None,
            knowledge: None,
            release_date: None,
            last_updated: None,
            modalities: Some(Modalities {
                input: vec![
                    "text".to_string(),
                    "image".to_string(),
                    "audio".to_string(),
                    "video".to_string(),
                ],
                output: vec!["text".to_string()],
            }),
            open_weights: None,
            cost: None,
            limit: None,
        };

        let actual: forge_domain::Model = fixture.into();

        assert_eq!(actual.input_modalities.len(), 4);
        assert!(actual.input_modalities.contains(&InputModality::Text));
        assert!(actual.input_modalities.contains(&InputModality::Image));
        assert!(actual.input_modalities.contains(&InputModality::Audio));
        assert!(actual.input_modalities.contains(&InputModality::Video));
    }
}
