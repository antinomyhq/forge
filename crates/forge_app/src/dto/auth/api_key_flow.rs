use std::collections::HashMap;

use super::{ApiKey, AuthFlow, URLParam, URLParamValue};

/// API key authentication flow
#[derive(Debug, Clone)]
pub struct ApiKeyAuthFlow;

/// Request parameters for API key authentication
#[derive(Debug, Clone)]
pub struct ApiKeyRequest {
    pub required_params: Vec<URLParam>,
}

/// Response containing API key and URL parameters
#[derive(Debug, Clone)]
pub struct ApiKeyResponse {
    pub api_key: ApiKey,
    pub url_params: HashMap<URLParam, URLParamValue>,
}

/// Method for API key authentication
#[derive(Debug, Clone)]
pub struct ApiKeyMethod;

impl AuthFlow for ApiKeyAuthFlow {
    type Request = ApiKeyRequest;
    type Response = ApiKeyResponse;
    type Method = ApiKeyMethod;
}
