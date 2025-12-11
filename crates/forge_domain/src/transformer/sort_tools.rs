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

    fn transform(&mut self, context: Self::Value) -> Self::Value {
        // Sort tools by name in alphabetical order
        // FIXME: Drop me
        // Since it's BTreeSet, always sorted.
        // context
        //     .tools
        //     .sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        context
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::ToolDefinition;

    fn fixture_context_with_tools() -> Context {
        Context::default().tools(
            vec![
                ToolDefinition::new("zebra_tool").description("Z tool"),
                ToolDefinition::new("alpha_tool").description("A tool"),
                ToolDefinition::new("beta_tool").description("B tool"),
            ]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        )
    }

    #[test]
    fn test_sorts_tools_by_name() {
        let fixture = fixture_context_with_tools();

        let mut transformer = SortTools::new();
        let actual = transformer.transform(fixture);

        let expected_order = vec!["alpha_tool", "beta_tool", "zebra_tool"];
        let actual_order: Vec<String> = actual
            .tools
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();

        assert_eq!(actual_order, expected_order);
    }
}
