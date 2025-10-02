use std::collections::HashMap;

use pretty_assertions::assert_eq;
use tokio::fs;

use super::*;

async fn read_fixture(filename: &str) -> String {
    let path = format!(
        "{}/src/dto/models_dev/fixtures/{}",
        env!("CARGO_MANIFEST_DIR"),
        filename
    );
    fs::read_to_string(&path)
        .await
        .expect(&format!("Failed to read fixture: {}", path))
}

fn expected_registry() -> ModelsDevRegistry {
    let mut registry = HashMap::new();

    let mut models = HashMap::new();

    models.insert(
        "test-model-1".to_string(),
        Model {
            id: "test-model-1".to_string(),
            name: "Test Model 1".to_string(),
            attachment: true,
            reasoning: true,
            temperature: true,
            tool_call: true,
            knowledge: Some("2024-10".to_string()),
            release_date: Some("2024-01-01".to_string()),
            last_updated: Some("2024-06-01".to_string()),
            modalities: Modalities {
                input: vec!["text".to_string(), "image".to_string()],
                output: vec!["text".to_string()],
            },
            open_weights: false,
            cost: Some(Cost {
                input: 2.5,
                output: 10.0,
                cache_read: Some(0.25),
                cache_write: Some(1.25),
                reasoning: Some(15.0),
            }),
            limit: Limit { context: 128000, output: 4096 },
        },
    );

    models.insert(
        "test-model-2".to_string(),
        Model {
            id: "test-model-2".to_string(),
            name: "Test Model 2".to_string(),
            attachment: false,
            reasoning: false,
            temperature: false,
            tool_call: false,
            knowledge: None,
            release_date: None,
            last_updated: None,
            modalities: Modalities {
                input: vec!["text".to_string()],
                output: vec!["text".to_string()],
            },
            open_weights: true,
            cost: None,
            limit: Limit { context: 8192, output: 2048 },
        },
    );

    registry.insert(
        "test-provider".to_string(),
        Provider {
            id: "test-provider".to_string(),
            env: vec!["TEST_API_KEY".to_string()],
            npm: Some("@ai-sdk/test".to_string()),
            api: Some("https://api.test.com/v1".to_string()),
            name: "Test Provider".to_string(),
            doc: Some("https://docs.test.com".to_string()),
            models,
        },
    );

    registry
}

#[tokio::test]
async fn test_deserialize_models_dev_registry() {
    let fixture = read_fixture("full_registry.json").await;
    let actual: ModelsDevRegistry = serde_json::from_str(&fixture).unwrap();
    let expected = expected_registry();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_serialize_models_dev_registry() {
    let fixture_json = read_fixture("full_registry.json").await;
    let fixture = expected_registry();
    let actual = serde_json::to_string_pretty(&fixture).unwrap();
    let expected: serde_json::Value = serde_json::from_str(&fixture_json).unwrap();
    let actual_json: serde_json::Value = serde_json::from_str(&actual).unwrap();
    assert_eq!(actual_json, expected);
}

#[tokio::test]
async fn test_model_with_minimal_fields() {
    let fixture = read_fixture("minimal_model.json").await;
    let actual: Model = serde_json::from_str(&fixture).unwrap();
    let expected = Model {
        id: "minimal-model".to_string(),
        name: "Minimal Model".to_string(),
        attachment: false,
        reasoning: false,
        temperature: false,
        tool_call: false,
        knowledge: None,
        release_date: None,
        last_updated: None,
        modalities: Modalities {
            input: vec!["text".to_string()],
            output: vec!["text".to_string()],
        },
        open_weights: true,
        cost: None,
        limit: Limit { context: 4096, output: 1024 },
    };

    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_provider_without_optional_fields() {
    let fixture = read_fixture("minimal_provider.json").await;
    let actual: Provider = serde_json::from_str(&fixture).unwrap();
    let expected = Provider {
        id: "minimal-provider".to_string(),
        env: vec![],
        npm: None,
        api: None,
        name: "Minimal Provider".to_string(),
        doc: None,
        models: HashMap::new(),
    };

    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_cost_with_all_fields() {
    let fixture = read_fixture("cost_all_fields.json").await;
    let actual: Cost = serde_json::from_str(&fixture).unwrap();
    let expected = Cost {
        input: 1.5,
        output: 3.0,
        cache_read: Some(0.5),
        cache_write: Some(0.75),
        reasoning: Some(5.0),
    };

    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_cost_with_minimal_fields() {
    let fixture = read_fixture("cost_minimal_fields.json").await;
    let actual: Cost = serde_json::from_str(&fixture).unwrap();
    let expected = Cost {
        input: 1.0,
        output: 2.0,
        cache_read: None,
        cache_write: None,
        reasoning: None,
    };

    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_multimodal_model() {
    let fixture = read_fixture("multimodal_model.json").await;
    let actual: Model = serde_json::from_str(&fixture).unwrap();

    assert_eq!(actual.modalities.input.len(), 3);
    assert_eq!(actual.modalities.output.len(), 2);
    assert!(actual.modalities.input.contains(&"text".to_string()));
    assert!(actual.modalities.input.contains(&"image".to_string()));
    assert!(actual.modalities.input.contains(&"audio".to_string()));
    assert!(actual.modalities.output.contains(&"text".to_string()));
    assert!(actual.modalities.output.contains(&"image".to_string()));
}
