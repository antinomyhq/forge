use super::{
    ApiKeyMethod, ApiKeyRequest, ApiKeyResponse, CodeMethod,
    CodeRequest, CodeResponse, DeviceCodeMethod, DeviceCodeRequest, DeviceCodeResponse,
};

#[derive(Debug, Clone)]
pub enum AuthContextRequest {
    ApiKey(ApiKeyRequest),
    DeviceCode(DeviceCodeRequest),
    Code(CodeRequest),
}

#[derive(Debug, Clone)]
pub struct FlowContext<Request, Response, Method> {
    pub request: Request,
    pub response: Response,
    pub method: Method,
}

#[derive(Debug, Clone)]
pub enum AuthContextResponse {
    ApiKey(FlowContext<ApiKeyRequest, ApiKeyResponse, ApiKeyMethod>),
    DeviceCode(FlowContext<DeviceCodeRequest, DeviceCodeResponse, DeviceCodeMethod>),
    Code(FlowContext<CodeRequest, CodeResponse, CodeMethod>),
}

impl AuthContextResponse {
    /// Creates an API key authentication context
    pub fn api_key(request: ApiKeyRequest, response: ApiKeyResponse, method: ApiKeyMethod) -> Self {
        Self::ApiKey(FlowContext { request, response, method })
    }

    /// Creates a device code authentication context
    pub fn device_code(
        request: DeviceCodeRequest,
        response: DeviceCodeResponse,
        method: DeviceCodeMethod,
    ) -> Self {
        Self::DeviceCode(FlowContext { request, response, method })
    }

    /// Creates an authorization code authentication context
    pub fn code(request: CodeRequest, response: CodeResponse, method: CodeMethod) -> Self {
        Self::Code(FlowContext { request, response, method })
    }
}
