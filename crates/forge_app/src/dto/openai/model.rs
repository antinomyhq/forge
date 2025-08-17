use forge_domain::ModelId;
use serde::{Deserialize, Deserializer, Serialize};

// Custom deserializer to handle both numeric and string pricing values
fn deserialize_price_value<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    use serde_json::Value;

    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        Some(Value::Number(n)) => {
            n.as_f64()
                .map(|f| Some(f as f32))
                .ok_or_else(|| Error::custom("invalid number for pricing value"))
        },
        Some(Value::String(s)) => {
            s.parse::<f32>()
                .map(Some)
                .map_err(|_| Error::custom("invalid string format for pricing value"))
        },
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(Error::custom(
            "expected number, string, or null for pricing value",
        )),
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Model {
    pub id: ModelId,
    pub name: Option<String>,
    pub created: Option<u64>,
    pub description: Option<String>,
    pub context_length: Option<u64>,
    pub architecture: Option<Architecture>,
    pub pricing: Option<Pricing>,
    pub top_provider: Option<TopProvider>,
    pub per_request_limits: Option<serde_json::Value>,
    pub supported_parameters: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Architecture {
    pub modality: String,
    pub tokenizer: String,
    pub instruct_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Pricing {
    #[serde(deserialize_with = "deserialize_price_value", default)]
    pub prompt: Option<f32>,
    #[serde(deserialize_with = "deserialize_price_value", default)]
    pub completion: Option<f32>,
    #[serde(deserialize_with = "deserialize_price_value", default)]
    pub image: Option<f32>,
    #[serde(deserialize_with = "deserialize_price_value", default)]
    pub request: Option<f32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TopProvider {
    pub context_length: Option<u64>,
    pub max_completion_tokens: Option<u64>,
    pub is_moderated: bool,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ListModelResponse {
    pub data: Vec<Model>,
}

impl From<Model> for forge_domain::Model {
    fn from(value: Model) -> Self {
        let tools_supported = value
            .supported_parameters
            .iter()
            .flatten()
            .any(|param| param == "tools");
        let supports_parallel_tool_calls = value
            .supported_parameters
            .iter()
            .flatten()
            .any(|param| param == "supports_parallel_tool_calls");
        let is_reasoning_supported = value
            .supported_parameters
            .iter()
            .flatten()
            .any(|param| param == "reasoning");

        forge_domain::Model {
            id: value.id,
            name: value.name,
            description: value.description,
            context_length: value.context_length,
            tools_supported: Some(tools_supported),
            supports_parallel_tool_calls: Some(supports_parallel_tool_calls),
            supports_reasoning: Some(is_reasoning_supported),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_deserialize_model_with_numeric_pricing() {
        // This reproduces the issue where Chutes API returns numeric pricing instead of
        // strings
        let fixture = serde_json::json!({
            "id": "moonshotai/Kimi-K2-Instruct-75k",
            "name": "Kimi K2 Instruct 75k",
            "pricing": {
                "prompt": 0.17992692,
                "completion": 0.17992692
            }
        });

        let actual = serde_json::from_value::<Model>(fixture);

        // This should not fail - we should be able to handle numeric pricing
        assert!(
            actual.is_ok(),
            "Should be able to deserialize model with numeric pricing: {:?}",
            actual.err()
        );
    }

    #[test]
    fn test_deserialize_model_with_string_pricing() {
        let fixture = serde_json::json!({
            "id": "test-model",
            "name": "Test Model",
            "pricing": {
                "prompt": "0.001",
                "completion": "0.002"
            }
        });

        let actual = serde_json::from_value::<Model>(fixture).unwrap();
        let expected = Model {
            id: "test-model".into(),
            name: Some("Test Model".to_string()),
            created: None,
            description: None,
            context_length: None,
            architecture: None,
            pricing: Some(Pricing {
                prompt: Some("0.001".to_string()),
                completion: Some("0.002".to_string()),
                image: None,
                request: None,
            }),
            top_provider: None,
            per_request_limits: None,
            supported_parameters: None,
        };

        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.name, expected.name);
        assert_eq!(
            actual.pricing.as_ref().unwrap().prompt,
            expected.pricing.as_ref().unwrap().prompt
        );
        assert_eq!(
            actual.pricing.as_ref().unwrap().completion,
            expected.pricing.as_ref().unwrap().completion
        );
    }
    #[test]
    fn test_deserialize_model_with_mixed_pricing() {
        // Test with mixed string, numeric, and null pricing values
        let fixture = serde_json::json!({
            "id": "mixed-model",
            "name": "Mixed Pricing Model",
            "pricing": {
                "prompt": "0.001",
                "completion": 0.002,
                "image": null,
                // request field is missing entirely
            }
        });

        let actual = serde_json::from_value::<Model>(fixture).unwrap();

        assert_eq!(
            actual.pricing.as_ref().unwrap().prompt,
            Some("0.001".to_string())
        );
        assert_eq!(
            actual.pricing.as_ref().unwrap().completion,
            Some("0.002".to_string())
        );
        assert_eq!(actual.pricing.as_ref().unwrap().image, None);
        assert_eq!(actual.pricing.as_ref().unwrap().request, None);
    }

    #[test]
    fn test_deserialize_model_without_pricing() {
        // Test that models without pricing field work correctly
        let fixture = serde_json::json!({
            "id": "no-pricing-model",
            "name": "No Pricing Model"
        });

        let actual = serde_json::from_value::<Model>(fixture).unwrap();

        assert_eq!(actual.id.as_str(), "no-pricing-model");
        assert_eq!(actual.name, Some("No Pricing Model".to_string()));
        assert_eq!(actual.pricing, None);
    }
    #[test]
    fn test_chutes_api_response_format() {
        // This simulates the actual Chutes API response format that was causing the issue
        let fixture = serde_json::json!({
            "data": [
                {
                    "id": "moonshotai/Kimi-K2-Instruct-75k",
                    "name": "Kimi K2 Instruct 75k",
                    "created": 1234567890,
                    "description": "Kimi K2 model with 75k context length",
                    "context_length": 75000,
                    "pricing": {
                        "prompt": 0.17992692,
                        "completion": 0.17992692
                    },
                    "supported_parameters": ["tools", "supports_parallel_tool_calls"]
                }
            ]
        });
        
        let actual = serde_json::from_value::<ListModelResponse>(fixture).unwrap();
        
        assert_eq!(actual.data.len(), 1);
        let model = &actual.data[0];
        assert_eq!(model.id.as_str(), "moonshotai/Kimi-K2-Instruct-75k");
        assert_eq!(model.name, Some("Kimi K2 Instruct 75k".to_string()));
        assert_eq!(model.context_length, Some(75000));
        
        let pricing = model.pricing.as_ref().unwrap();
        assert_eq!(pricing.prompt, Some("0.17992692".to_string()));
        assert_eq!(pricing.completion, Some("0.17992692".to_string()));
        assert_eq!(pricing.image, None);
        assert_eq!(pricing.request, None);
    }
}
