use std::marker::PhantomData;

/// Trait for type-safe authentication flows
///
/// Ensures request and response types are correctly paired at compile time
pub trait AuthFlow: Sized {
    type Request: Clone;
    type Response: Clone;
    type Method: Clone;
}

/// Flow context containing request, response, and method for an authentication
/// flow
#[derive(Debug, Clone)]
pub struct FlowContext<T: AuthFlow> {
    pub request: T::Request,
    pub response: T::Response,
    pub method: T::Method,
    _marker: PhantomData<T>,
}

impl<T: AuthFlow> FlowContext<T> {
    pub fn new(request: T::Request, response: T::Response, method: T::Method) -> Self {
        Self { request, response, method, _marker: PhantomData }
    }
}
