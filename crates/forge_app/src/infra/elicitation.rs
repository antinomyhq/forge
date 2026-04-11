//! Elicitation dispatcher trait for MCP server-initiated user prompts.
//!
//! When an MCP server sends an `elicitation/create` request (per the MCP spec),
//! the rmcp `ClientHandler::create_elicitation` callback needs to route the
//! request somewhere. This trait is that somewhere — the `forge_infra`
//! crate will implement a `ForgeMcpHandler` that forwards rmcp's raw
//! request into a call on `ElicitationDispatcher`, which in turn fires
//! the `Elicitation` plugin hook, inspects the result for an auto-
//! response, and falls back to interactive UI when no hook handles it.
//!
//! Currently, a non-hook-handled request returns `ElicitationAction::Decline`.

use async_trait::async_trait;
use serde_json::Value;

/// A server-originated elicitation request.
///
/// Mirrors `rmcp::model::CreateElicitationRequestParam` but uses plain
/// types (no rmcp dep in `forge_app`) so `forge_app` stays decoupled
/// from the transport layer. A translation layer in `forge_infra`
/// converts rmcp types to these.
#[derive(Debug, Clone)]
pub struct ElicitationRequest {
    /// The logical name of the MCP server that sent the request. Used
    /// as the `matcher` value in hook configs so plugins can target
    /// specific servers.
    pub server_name: String,
    /// The user-facing message the server wants to show.
    pub message: String,
    /// The JSON Schema describing the expected response shape. Present
    /// in form mode; `None` in url mode.
    pub requested_schema: Option<Value>,
    /// Presence of this field indicates url mode; the URL the client
    /// should open in the user's default browser.
    pub url: Option<String>,
}

/// The user's (or plugin's) response to an elicitation request.
///
/// Mirrors `rmcp::model::CreateElicitationResult` with a translation
/// layer in `forge_infra`.
#[derive(Debug, Clone)]
pub struct ElicitationResponse {
    /// Accept / Decline / Cancel per the MCP spec.
    pub action: ElicitationAction,
    /// The filled-in form data when action is Accept in form mode.
    /// Always `None` for url-mode responses.
    pub content: Option<Value>,
}

/// The set of actions the user (or a plugin) may return for an
/// elicitation request, per the MCP elicitation spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElicitationAction {
    Accept,
    Decline,
    Cancel,
}

impl ElicitationAction {
    /// Wire-format string matching Claude Code's action vocabulary
    /// (`accept` / `decline` / `cancel`). Used when fanning the
    /// response out to the `ElicitationResult` hook payload.
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Cancel => "cancel",
        }
    }
}

/// Trait for handling MCP elicitation requests.
///
/// Implementors typically:
/// 1. Fire the `Elicitation` plugin hook via `fire_elicitation_hook`.
/// 2. Inspect the resulting `AggregatedHookResult` for auto-response:
///    - `blocking_error` → return Cancel
///    - `permission_behavior == Deny` → return Decline
///    - `permission_behavior == Allow` + `updated_input` → return Accept with
///      the plugin-provided form data
/// 3. Fall back to interactive UI when no hook handles the request.
/// 4. Fire `ElicitationResult` hook after the user responds (or the plugin
///    short-circuit path).
#[async_trait]
pub trait ElicitationDispatcher: Send + Sync {
    /// Dispatch an elicitation request and return the user/plugin
    /// response.
    async fn elicit(&self, request: ElicitationRequest) -> ElicitationResponse;
}
