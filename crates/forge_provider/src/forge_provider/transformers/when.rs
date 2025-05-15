use super::Transformer;
use crate::forge_provider::request::Request;

/// A transformer that conditionally applies another transformer based on a
/// predicate. The condition is checked before applying the transformation, and
/// if it returns false, the request is passed through unchanged.
pub struct When<T, F>(pub(super) T, pub(super) F);

impl<T: Transformer, F: Fn(&Request) -> bool> Transformer for When<T, F> {
    fn transform(&self, request: Request) -> Request {
        if (self.1)(&request) {
            self.0.transform(request)
        } else {
            request
        }
    }
}
