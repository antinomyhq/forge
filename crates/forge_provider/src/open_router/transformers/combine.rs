use super::transformer::Transformer;
use crate::open_router::request::OpenRouterRequest;

/// A transformer that combines two transformers, applying them in sequence.
/// The transformations are applied in the order: B then A (right to left).
#[derive(Debug)]
pub(crate) struct Combine<A, B>(pub(super) A, pub(super) B);

impl<A: Transformer, B: Transformer> Transformer for Combine<A, B> {
    fn transform(&self, request: OpenRouterRequest) -> OpenRouterRequest {
        self.0.transform(self.1.transform(request))
    }
}
