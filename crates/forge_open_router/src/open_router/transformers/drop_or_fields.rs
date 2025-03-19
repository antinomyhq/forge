use super::Transformer;
use crate::open_router::request::OpenRouterRequest;

/// makes the OpenRouterRequest compatible with the OpenAI API.
pub struct DropOpenRouterFields;

impl Transformer for DropOpenRouterFields {
    fn transform(&self, mut request: OpenRouterRequest) -> OpenRouterRequest {
        // remove fields that are not supported by open-ai.
        request.provider = None;
        request.transforms = None;
        request.prompt = None;
        request.models = None;
        request.route = None;
        request.top_k = None;
        request.repetition_penalty = None;
        request.min_p = None;
        request.top_a = None;

        let tools_present =
            request
                .tools
                .as_ref()
                .and_then(|tools| if !tools.is_empty() { Some(true) } else { None });
        if tools_present.is_none() {
            // drop `parallel_tool_calls` field if tools are not passed to the request.
            request.parallel_tool_calls = None;
        }
        request
    }
}
