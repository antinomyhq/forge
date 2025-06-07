use super::Transformer;
use crate::forge_provider::request::Request;

/// makes the Request compatible with the OpenAI API.
pub struct MakeOpenAiCompat;

impl Transformer for MakeOpenAiCompat {
    fn transform(&self, mut request: Request) -> Request {
        // remove fields that are not supported by open-ai.
        request.provider = None;
        request.transforms = None;
        request.prompt = None;
        request.models = None;
        request.route = None;
        request.top_k = None;
        request.top_p = None;
        request.repetition_penalty = None;
        request.min_p = None;
        request.top_a = None;
        request.session_id = None;

        let tools_present = request
            .tools
            .as_ref()
            .is_some_and(|tools| !tools.is_empty());

        if tools_present {
            // drop `parallel_tool_calls` field if tools are not passed to the request.
            request.parallel_tool_calls = None;
        }
        request
    }
}
