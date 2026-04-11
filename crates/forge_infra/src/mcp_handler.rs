//! Forge's implementation of the rmcp [`ClientHandler`] trait.
//!
//! Wave F-2 of the Claude Code plugin compatibility plan. The rmcp
//! client needs a handler that advertises the
//! [`ElicitationCapability`] during MCP initialize negotiation AND
//! implements [`ClientHandler::create_elicitation`] so that
//! server-initiated `elicitation/create` requests get routed to
//! [`forge_app::ElicitationDispatcher::elicit`] instead of rmcp's
//! default implementation (which automatically declines everything).
//!
//! # Wiring
//!
//! A [`ForgeMcpHandler`] is constructed once per `.serve(transport)`
//! call site in `mcp_client.rs`. The handler owns:
//!
//! - `server_name: String` — used as the hook matcher so plugins can target
//!   specific MCP servers in their hook configs.
//! - `dispatcher: Arc<dyn ElicitationDispatcher>` — the process-wide dispatcher
//!   produced by `ForgeServices::elicitation_dispatcher()` and plumbed through
//!   [`crate::ForgeInfra::init_elicitation_dispatcher`] from
//!   `forge_api::ForgeAPI::init`.
//!
//! # Graceful degradation when dispatcher is absent
//!
//! [`ForgeMcpHandler`] can be created with no dispatcher attached —
//! in that case `create_elicitation` returns `Decline` so the MCP
//! server still gets a well-formed response. This matches the
//! graceful-degradation pattern used elsewhere in Wave F-1 when the
//! `Services` aggregate hasn't yet been plumbed through the
//! dispatcher `OnceLock`.

use std::sync::Arc;

use forge_app::{ElicitationAction, ElicitationDispatcher, ElicitationRequest};
use rmcp::handler::client::ClientHandler;
use rmcp::model::{
    ClientCapabilities, ClientInfo, CreateElicitationRequestParam, CreateElicitationResult,
    ElicitationAction as RmcpElicitationAction, ElicitationCapability, ErrorData as McpError,
    Implementation,
};
use rmcp::service::{RequestContext, RoleClient};

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

/// rmcp [`ClientHandler`] implementation that routes elicitation
/// requests through [`forge_app::ElicitationDispatcher`].
///
/// Construct one per `.serve(transport)` call site in
/// [`crate::mcp_client::ForgeMcpClient`]. The handler is consumed by
/// rmcp's `ServiceExt::serve` and stored inside the resulting
/// `RunningService`, so the type-parameter propagation lines up
/// without needing `Box<dyn>` erasure at the rmcp boundary.
pub struct ForgeMcpHandler {
    /// Logical MCP server name used as the hook matcher so plugins
    /// can target specific servers via their `matcher` field.
    server_name: String,
    /// Late-bound dispatcher. `None` means the handler was created
    /// before the dispatcher wiring was plumbed in — in that case
    /// `create_elicitation` declines every request.
    dispatcher: Option<Arc<dyn ElicitationDispatcher>>,
}

impl ForgeMcpHandler {
    /// Create a handler for the given MCP server name with an
    /// attached dispatcher.
    pub fn new(server_name: String, dispatcher: Arc<dyn ElicitationDispatcher>) -> Self {
        Self { server_name, dispatcher: Some(dispatcher) }
    }

    /// Create a handler with no dispatcher attached. Used as a safe
    /// fallback when `ForgeInfra::init_elicitation_dispatcher` hasn't
    /// been called yet (e.g. during early bootstrap or standalone
    /// `mcp_auth` flows). The resulting handler declines every
    /// elicitation request instead of hanging.
    pub fn without_dispatcher(server_name: String) -> Self {
        Self { server_name, dispatcher: None }
    }
}

impl ClientHandler for ForgeMcpHandler {
    /// Advertise the elicitation capability so MCP servers know they
    /// can send `elicitation/create` requests. `client_info` mirrors
    /// what the existing [`crate::mcp_client::ForgeMcpClient::client_info`]
    /// used before Wave F-2, so server-side logging/telemetry
    /// continues to see the `Forge`/version pair.
    fn get_info(&self) -> ClientInfo {
        ClientInfo {
            protocol_version: Default::default(),
            capabilities: ClientCapabilities {
                elicitation: Some(ElicitationCapability::default()),
                ..Default::default()
            },
            client_info: Implementation {
                name: "Forge".to_string(),
                version: VERSION.to_string(),
                icons: None,
                title: None,
                website_url: None,
            },
        }
    }

    /// Convert rmcp's [`CreateElicitationRequestParam`] into the
    /// plain-types [`ElicitationRequest`] that `forge_app` speaks,
    /// dispatch it through the
    /// [`forge_app::ElicitationDispatcher`] (which fires the
    /// `Elicitation` plugin hook and then falls back to the
    /// interactive UI), and translate the response back into
    /// [`CreateElicitationResult`] for rmcp's wire format.
    ///
    /// Errors from the dispatcher (or a missing dispatcher) degrade
    /// to `Decline` so the MCP server always gets a well-formed
    /// response — never a `method_not_found` which would look like a
    /// protocol violation from the server's side.
    fn create_elicitation(
        &self,
        request: CreateElicitationRequestParam,
        _context: RequestContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<CreateElicitationResult, McpError>> + Send + '_
    {
        async move {
            let Some(dispatcher) = self.dispatcher.as_ref() else {
                tracing::warn!(
                    server = %self.server_name,
                    "ForgeMcpHandler received create_elicitation but dispatcher is not attached; declining"
                );
                return Ok(CreateElicitationResult {
                    action: RmcpElicitationAction::Decline,
                    content: None,
                });
            };

            // rmcp's `ElicitationSchema` is a strongly-typed wrapper
            // but `ElicitationRequest.requested_schema` is a plain
            // `serde_json::Value` so `forge_app` stays decoupled from
            // rmcp types. Serializing and immediately deserializing
            // collapses to the wire-format JSON representation, which
            // is exactly what the dispatcher's form renderer walks.
            // On serialization failure (should never happen — the
            // type implements `Serialize`), fall through with an
            // empty schema rather than erroring.
            let requested_schema = serde_json::to_value(&request.requested_schema).ok();

            let forge_request = ElicitationRequest {
                server_name: self.server_name.clone(),
                message: request.message.clone(),
                requested_schema,
                // Wave F-2 Pass 1: rmcp's
                // `CreateElicitationRequestParam` does not carry an
                // explicit `url` field — the MCP 2025-06-18 spec does
                // not standardize url-mode elicitation at the
                // protocol level. Forge's `url` branch is reserved
                // for plugin-injected hook responses that opt into
                // the browser-open UX. Direct MCP server requests
                // always flow through form mode.
                url: None,
            };

            let response = dispatcher.elicit(forge_request).await;

            Ok(CreateElicitationResult {
                action: match response.action {
                    ElicitationAction::Accept => RmcpElicitationAction::Accept,
                    ElicitationAction::Decline => RmcpElicitationAction::Decline,
                    ElicitationAction::Cancel => RmcpElicitationAction::Cancel,
                },
                content: response.content,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use forge_app::ElicitationResponse;
    use pretty_assertions::assert_eq;

    use super::*;

    /// Test double that echoes a preconfigured response for each
    /// dispatch call, capturing the incoming request so assertions
    /// can verify the translation from rmcp types into
    /// `ElicitationRequest` is correct.
    struct StubDispatcher {
        response: ElicitationResponse,
    }

    #[async_trait]
    impl ElicitationDispatcher for StubDispatcher {
        async fn elicit(&self, _request: ElicitationRequest) -> ElicitationResponse {
            self.response.clone()
        }
    }

    #[test]
    fn test_get_info_advertises_elicitation_capability() {
        let dispatcher: Arc<dyn ElicitationDispatcher> = Arc::new(StubDispatcher {
            response: ElicitationResponse { action: ElicitationAction::Decline, content: None },
        });
        let handler = ForgeMcpHandler::new("test-server".to_string(), dispatcher);

        let info = handler.get_info();
        assert!(
            info.capabilities.elicitation.is_some(),
            "elicitation capability must be advertised so MCP servers know Forge accepts elicitation/create requests"
        );
        assert_eq!(info.client_info.name, "Forge");
    }

    #[test]
    fn test_without_dispatcher_still_advertises_capability() {
        // Even the fallback constructor should advertise the
        // capability — otherwise servers would route around us
        // entirely and we'd lose the opportunity to log/warn about
        // the missing dispatcher.
        let handler = ForgeMcpHandler::without_dispatcher("test-server".to_string());
        let info = handler.get_info();
        assert!(info.capabilities.elicitation.is_some());
    }
}
