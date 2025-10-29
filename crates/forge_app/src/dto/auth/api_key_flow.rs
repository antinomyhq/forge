use std::collections::HashMap;

use super::{ApiKey, URLParam, URLParamValue};

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
