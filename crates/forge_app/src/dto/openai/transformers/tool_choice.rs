use forge_domain::Transformer;

use crate::dto::openai::{Request, ToolChoice};

pub struct SetToolChoice {
    choice: ToolChoice,
}

impl SetToolChoice {
    pub fn new(choice: ToolChoice) -> Self {
        Self { choice }
    }
}

impl Transformer for SetToolChoice {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        request.tool_choice = Some(self.choice.clone());
        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Context, ModelId};

    use super::*;

    #[test]
    fn test_gemini_transformer_tool_strategy() {
        let context = Context::default();
        let request = Request::from(context).model(ModelId::new("google/gemini-pro"));

        let transformer = SetToolChoice::new(ToolChoice::Auto);
        let mut transformer = transformer;
        let transformed = transformer.transform(request);

        assert_eq!(transformed.tool_choice, Some(ToolChoice::Auto));
    }
}
