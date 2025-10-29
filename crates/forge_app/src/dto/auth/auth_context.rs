use std::collections::HashMap;

use super::{
    ApiKeyRequest, ApiKeyResponse, CodeRequest, CodeResponse, DeviceCodeRequest, DeviceCodeResponse,
};

#[derive(Debug, Clone)]
pub enum AuthContextRequest {
    ApiKey(ApiKeyRequest),
    DeviceCode(DeviceCodeRequest),
    Code(CodeRequest),
}

#[derive(Debug, Clone)]
pub struct FlowContext<Request, Response> {
    pub request: Request,
    pub response: Response,
}

#[derive(Debug, Clone)]
pub enum AuthContextResponse {
    ApiKey(FlowContext<ApiKeyRequest, ApiKeyResponse>),
    DeviceCode(FlowContext<DeviceCodeRequest, DeviceCodeResponse>),
    Code(FlowContext<CodeRequest, CodeResponse>),
}

impl AuthContextResponse {
    /// Creates an API key authentication context
    pub fn api_key(
        request: ApiKeyRequest,
        api_key: impl ToString,
        url_params: HashMap<String, String>,
    ) -> Self {
        Self::ApiKey(FlowContext {
            request,
            response: ApiKeyResponse {
                api_key: api_key.to_string().into(),
                url_params: url_params
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            },
        })
    }

    /// Creates a device code authentication context
    pub fn device_code(request: DeviceCodeRequest) -> Self {
        Self::DeviceCode(FlowContext { request, response: DeviceCodeResponse })
    }

    /// Creates an authorization code authentication context
    pub fn code(request: CodeRequest, code: impl ToString) -> Self {
        Self::Code(FlowContext {
            request,
            response: CodeResponse { code: code.to_string().into() },
        })
    }
}
