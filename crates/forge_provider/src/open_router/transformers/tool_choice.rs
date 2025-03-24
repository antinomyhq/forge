use crate::open_router::request::OpenRouterRequest;
use crate::open_router::tool_choice::ToolChoice;
use crate::open_router::transformers::Transformer;

pub struct SetToolChoice {
    choice: ToolChoice,
}

impl SetToolChoice {
    pub fn new(choice: ToolChoice) -> Self {
        Self { choice }
    }
}

impl Transformer for SetToolChoice {
    fn transform(&self, mut request: OpenRouterRequest) -> OpenRouterRequest {
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
        let request = OpenRouterRequest::from(context).model(ModelId::new("google/gemini-pro"));

        let transformer = SetToolChoice::new(ToolChoice::Auto);
        let transformed = transformer.transform(request);

        assert_eq!(transformed.tool_choice, Some(ToolChoice::Auto));
    }
}
