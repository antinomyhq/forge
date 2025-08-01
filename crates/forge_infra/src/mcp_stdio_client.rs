use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::time::Duration;

use anyhow::{Context, anyhow};
use rmcp::model::{
    CallToolRequest, CallToolResult, Extensions, InitializeRequest, InitializeRequestParam,
    InitializeResult, InitializedNotification, JsonRpcError, JsonRpcMessage, JsonRpcNotification,
    JsonRpcRequest, JsonRpcResponse, JsonRpcVersion2_0, ListToolsRequest, ListToolsResult,
    Notification, RequestId,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time;

const CHANNEL_CAPACITY: usize = 128;

type PendingSender = oneshot::Sender<JsonRpcMessage>;

pub trait ModelContextProtocolRequest {
    const METHOD: &'static str;
    type Params: DeserializeOwned + Serialize + Send + Sync + 'static;
    type Result: DeserializeOwned + Serialize + Send + Sync + 'static;
}

pub trait ModelContextProtocolNotification {
    const METHOD: &'static str;
    type Params: DeserializeOwned + Serialize + Send + Sync + 'static;
}

impl ModelContextProtocolRequest for InitializeRequest {
    const METHOD: &'static str = "initialize";
    type Params = InitializeRequestParam;
    type Result = InitializeResult;
}

impl ModelContextProtocolNotification for InitializedNotification {
    const METHOD: &'static str = "notifications/initialized";
    type Params = Option<serde_json::Value>;
}

impl ModelContextProtocolRequest for ListToolsRequest {
    const METHOD: &'static str = "tools/list";
    type Params = Option<ListToolsRequestParams>;
    type Result = ListToolsResult;
}

impl ModelContextProtocolRequest for CallToolRequest {
    const METHOD: &'static str = "tools/call";
    type Params = CallToolRequestParams;
    type Result = CallToolResult;
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ListToolsRequestParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CallToolRequestParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    pub name: String,
}

#[derive(Debug)]
pub struct McpClient {
    child: tokio::process::Child,
    outgoing_tx: mpsc::Sender<JsonRpcMessage>,
    pending: Arc<Mutex<HashMap<i64, PendingSender>>>,
    id_counter: AtomicI64,
}

impl McpClient {
    pub async fn new_stdio_client(
        program: OsString,
        args: Vec<OsString>,
        env: Option<HashMap<String, String>>,
    ) -> std::io::Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(create_env_for_mcp_server(env))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("failed to capture child stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("failed to capture child stdout"))?;

        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<JsonRpcMessage>(CHANNEL_CAPACITY);
        let pending: Arc<Mutex<HashMap<i64, PendingSender>>> = Arc::new(Mutex::new(HashMap::new()));

        let writer_handle = {
            let mut stdin = stdin;
            tokio::spawn(async move {
                while let Some(msg) = outgoing_rx.recv().await {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if stdin.write_all(json.as_bytes()).await.is_err() {
                            break;
                        }
                        if stdin.write_all(b"\n").await.is_err() {
                            break;
                        }
                        if stdin.flush().await.is_err() {
                            break;
                        }
                    }
                }
            })
        };

        let reader_handle = {
            let pending = pending.clone();
            let mut lines = BufReader::new(stdout).lines();

            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    match serde_json::from_str::<JsonRpcMessage>(&line) {
                        Ok(JsonRpcMessage::Response(resp)) => {
                            Self::dispatch_response(resp, &pending).await;
                        }
                        Ok(JsonRpcMessage::Error(err)) => {
                            Self::dispatch_error(err, &pending).await;
                        }
                        Ok(_) => {}
                        Err(_) => {}
                    }
                }
            })
        };

        let _ = (writer_handle, reader_handle);

        Ok(Self { child, outgoing_tx, pending, id_counter: AtomicI64::new(1) })
    }

    pub async fn send_request<R>(
        &self,
        params: R::Params,
        timeout: Option<Duration>,
    ) -> anyhow::Result<R::Result>
    where
        R: ModelContextProtocolRequest,
        R::Params: Serialize,
        R::Result: DeserializeOwned,
    {
        let id = self
            .id_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let request_id = RequestId::Number(id.try_into().unwrap());

        let params_json = serde_json::to_value(&params)?;
        let params_field = if params_json.is_null() {
            serde_json::Map::new()
        } else {
            params_json.as_object().unwrap().clone()
        };

        let jsonrpc_request = JsonRpcRequest {
            id: request_id.clone(),
            jsonrpc: JsonRpcVersion2_0,
            request: rmcp::model::Request {
                method: R::METHOD.to_string(),
                params: params_field,
                extensions: Extensions::default(),
            },
        };

        let message = JsonRpcMessage::Request(jsonrpc_request);

        let (tx, rx) = oneshot::channel();

        {
            let mut guard = self.pending.lock().await;
            guard.insert(id, tx);
        }

        if self.outgoing_tx.send(message).await.is_err() {
            return Err(anyhow!(
                "failed to send message to writer task - channel closed"
            ));
        }

        let msg = match timeout {
            Some(duration) => match time::timeout(duration, rx).await {
                Ok(Ok(msg)) => msg,
                Ok(Err(_)) => {
                    let mut guard = self.pending.lock().await;
                    guard.remove(&id);
                    return Err(anyhow!(
                        "response channel closed before a reply was received"
                    ));
                }
                Err(_) => {
                    let mut guard = self.pending.lock().await;
                    guard.remove(&id);
                    return Err(anyhow!("request timed out"));
                }
            },
            None => rx
                .await
                .map_err(|_| anyhow!("response channel closed before a reply was received"))?,
        };

        match msg {
            JsonRpcMessage::Response(JsonRpcResponse { result, .. }) => {
                let typed: R::Result = serde_json::from_value(serde_json::Value::Object(result))?;
                Ok(typed)
            }
            JsonRpcMessage::Error(err) => Err(anyhow!(format!(
                "server returned JSON-RPC error: code = {:?}, message = {}",
                err.error.code, err.error.message
            ))),
            other => Err(anyhow!(format!(
                "unexpected message variant received in reply path: {:?}",
                other
            ))),
        }
    }

    pub async fn send_notification<N>(&self, params: N::Params) -> anyhow::Result<()>
    where
        N: ModelContextProtocolNotification,
        N::Params: Serialize,
    {
        let params_json = serde_json::to_value(&params)?;
        let params_field = if params_json.is_null() {
            serde_json::Map::new()
        } else {
            params_json.as_object().unwrap().clone()
        };

        let method = N::METHOD.to_string();
        let jsonrpc_notification = JsonRpcNotification {
            jsonrpc: JsonRpcVersion2_0,
            notification: Notification {
                method: method.clone(),
                params: params_field,
                extensions: Extensions::default(),
            },
        };

        let notification = JsonRpcMessage::Notification(jsonrpc_notification);
        self.outgoing_tx
            .send(notification)
            .await
            .with_context(|| format!("failed to send notification `{method}` to writer task"))
    }

    pub async fn initialize(
        &self,
        initialize_params: InitializeRequestParam,
        initialize_notification_params: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> anyhow::Result<InitializeResult> {
        let response = self
            .send_request::<InitializeRequest>(initialize_params, timeout)
            .await?;
        self.send_notification::<InitializedNotification>(initialize_notification_params)
            .await?;
        Ok(response)
    }

    pub async fn list_tools(
        &self,
        params: Option<ListToolsRequestParams>,
        timeout: Option<Duration>,
    ) -> anyhow::Result<ListToolsResult> {
        self.send_request::<ListToolsRequest>(params, timeout).await
    }

    pub async fn call_tool(
        &self,
        name: String,
        arguments: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> anyhow::Result<CallToolResult> {
        let params = CallToolRequestParams { name, arguments };
        self.send_request::<CallToolRequest>(params, timeout).await
    }

    async fn dispatch_response(
        resp: JsonRpcResponse,
        pending: &Arc<Mutex<HashMap<i64, PendingSender>>>,
    ) {
        let id = match resp.id {
            RequestId::Number(i) => i,
            RequestId::String(_) => {
                return;
            }
        };

        if let Some(tx) = pending.lock().await.remove(&(id as i64)) {
            let _ = tx.send(JsonRpcMessage::Response(resp));
        }
    }

    async fn dispatch_error(err: JsonRpcError, pending: &Arc<Mutex<HashMap<i64, PendingSender>>>) {
        let id = match err.id {
            RequestId::Number(i) => i,
            RequestId::String(_) => return,
        };

        if let Some(tx) = pending.lock().await.remove(&(id as i64)) {
            let _ = tx.send(JsonRpcMessage::Error(err));
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.try_wait();
    }
}

fn create_env_for_mcp_server(
    extra_env: Option<HashMap<String, String>>,
) -> HashMap<String, String> {
    DEFAULT_ENV_VARS
        .iter()
        .filter_map(|var| match std::env::var(var) {
            Ok(value) => Some((var.to_string(), value)),
            Err(_) => None,
        })
        .chain(extra_env.unwrap_or_default())
        .collect::<HashMap<_, _>>()
}

#[rustfmt::skip]
#[cfg(unix)]
const DEFAULT_ENV_VARS: &[&str] = &[
    "__CF_USER_TEXT_ENCODING",
    "HOME",
    "LANG",
    "LC_ALL",
    "LOGNAME",
    "PATH",
    "SHELL",
    "TERM",
    "TMPDIR",
    "TZ",
    "USER",
];

#[cfg(windows)]
const DEFAULT_ENV_VARS: &[&str] = &[
    "PATH",
    "PATHEXT",
    "TEMP",
    "TMP",
    "USERDOMAIN",
    "USERNAME",
    "USERPROFILE",
];
