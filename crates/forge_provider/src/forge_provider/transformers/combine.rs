use super::transformer::Transformer;
use crate::forge_provider::request::Request;

/// A transformer that combines two transformers, applying them in sequence.
/// The transformations are applied in the order: B then A (right to left).
#[derive(Debug)]
pub(crate) struct Combine<A, B>(pub(super) A, pub(super) B);

impl<A: Transformer, B: Transformer> Transformer for Combine<A, B> {
    fn transform(&self, request: Request) -> Request {
        self.0.transform(self.1.transform(request))
    }
}
