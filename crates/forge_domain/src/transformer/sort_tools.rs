use super::Transformer;
use crate::Context;

/// Transformer that sorts tools in the context alphabetically by their name
pub struct SortTools;

impl SortTools {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SortTools {
    fn default() -> Self {
        Self::new()
    }
}

impl Transformer for SortTools {
    type Value = Context;

    fn transform(&mut self, mut context: Self::Value) -> Self::Value {
        // Sort tools by name in alphabetical order
        context
            .tools
            .sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        context
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_yaml_snapshot;
    use pretty_assertions::assert_eq;
    use serde::Serialize;

    use super::*;
    use crate::ToolDefinition;

    #[derive(Serialize)]
    struct TransformationSnapshot {
        transformation: String,
        before: Context,
        after: Context,
    }

    impl TransformationSnapshot {
        fn new(transformation: &str, before: Context, after: Context) -> Self {
            Self { transformation: transformation.to_string(), before, after }
        }
    }

    fn fixture_context_with_tools() -> Context {
        Context::default().tools(vec![
            ToolDefinition::new("zebra_tool").description("Z tool"),
            ToolDefinition::new("alpha_tool").description("A tool"),
            ToolDefinition::new("beta_tool").description("B tool"),
        ])
    }

    #[test]
    fn test_sorts_tools_by_name() {
        let fixture = fixture_context_with_tools();

        let mut transformer = SortTools::new();
        let actual = transformer.transform(fixture.clone());

        let expected_order = vec!["alpha_tool", "beta_tool", "zebra_tool"];
        let actual_order: Vec<String> = actual
            .tools
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();

        assert_eq!(actual_order, expected_order);

        let snapshot = TransformationSnapshot::new("SortTools", fixture, actual);
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn test_handles_empty_tools() {
        let fixture = Context::default();

        let mut transformer = SortTools::new();
        let actual = transformer.transform(fixture.clone());
        let expected = fixture;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_handles_single_tool() {
        let fixture = Context::default().tools(vec![
            ToolDefinition::new("single_tool").description("Single tool"),
        ]);

        let mut transformer = SortTools::new();
        let actual = transformer.transform(fixture.clone());

        let expected_order = vec!["single_tool"];
        let actual_order: Vec<String> = actual
            .tools
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();

        assert_eq!(actual_order, expected_order);
    }
}
