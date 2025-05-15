use super::Transformer;
use crate::forge_provider::request::Request;

/// A transformer that returns the request unchanged
#[derive(Default)]
pub struct Identity;

impl Transformer for Identity {
    fn transform(&self, request: Request) -> Request {
        request
    }
}
