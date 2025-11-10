//!
//! Follows the design specifications of Claude's [.mcp.json](https://docs.anthropic.com/en/docs/claude-code/tutorials#set-up-model-context-protocol-mcp)

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

use derive_more::{Deref, Display, From};
use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    Local,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
#[serde(untagged)]
pub enum McpServerConfig {
    Stdio(McpStdioServer),
    Sse(McpSseServer),
}

impl McpServerConfig {
    /// Create a new stdio-based MCP server
    pub fn new_stdio(
        command: impl Into<String>,
        args: Vec<String>,
        env: Option<BTreeMap<String, String>>,
    ) -> Self {
        Self::Stdio(McpStdioServer {
            command: command.into(),
            args,
            env: env.unwrap_or_default(),
            disable: false,
        })
    }

    /// Create a new SSE-based MCP server
    pub fn new_sse(url: impl Into<String>) -> Self {
        Self::Sse(McpSseServer { url: url.into(), headers: BTreeMap::new(), disable: false })
    }

    /// Create a new SSE-based MCP server with headers
    pub fn new_sse_with_headers(url: impl Into<String>, headers: BTreeMap<String, String>) -> Self {
        Self::Sse(McpSseServer { url: url.into(), headers, disable: false })
    }

    pub fn is_disabled(&self) -> bool {
        match self {
            McpServerConfig::Stdio(v) => v.disable,
            McpServerConfig::Sse(v) => v.disable,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Setters, PartialEq, Hash)]
#[setters(strip_option, into)]
pub struct McpStdioServer {
    /// Command to execute for starting this MCP server
    #[serde(skip_serializing_if = "String::is_empty")]
    pub command: String,

    /// Arguments to pass to command
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables to pass to command
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,

    /// Disable it temporarily without having to
    /// remove it from config.
    #[serde(default)]
    pub disable: bool,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct McpSseServer {
    /// Url of the MCP server
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,

    /// HTTP headers to send with the request
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,

    /// Disable it temporarily without having to
    /// remove it from config.
    #[serde(default)]
    pub disable: bool,
}

impl Display for McpServerConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut output = String::new();
        match self {
            McpServerConfig::Stdio(stdio) => {
                output.push_str(&format!("{} ", stdio.command));
                stdio.args.iter().for_each(|arg| {
                    output.push_str(&format!("{arg} "));
                });

                stdio.env.iter().for_each(|(key, value)| {
                    output.push_str(&format!("{key}={value} "));
                });
            }
            McpServerConfig::Sse(sse) => {
                output.push_str(&format!("{} ", sse.url));
                for (key, value) in &sse.headers {
                    output.push_str(&format!("{}:{} ", key, value));
                }
            }
        }

        write!(f, "{}", output.trim())
    }
}

#[derive(
    Clone, Display, Serialize, Deserialize, Debug, PartialEq, Hash, Eq, From, PartialOrd, Ord, Deref,
)]
pub struct ServerName(String);

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Merge)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpConfig {
    #[merge(strategy = std::collections::BTreeMap::extend)]
    #[serde(default)]
    pub mcp_servers: BTreeMap<ServerName, McpServerConfig>,
}

impl Deref for McpConfig {
    type Target = BTreeMap<ServerName, McpServerConfig>;

    fn deref(&self) -> &Self::Target {
        &self.mcp_servers
    }
}

impl From<BTreeMap<ServerName, McpServerConfig>> for McpConfig {
    fn from(mcp_servers: BTreeMap<ServerName, McpServerConfig>) -> Self {
        Self { mcp_servers }
    }
}

impl McpConfig {
    /// Compute a deterministic u64 identifier for this config
    ///
    /// Uses Rust's built-in `Hash` trait (derived) to compute a stable hash
    /// and converts it to a hex u64 for use as a cache key.
    /// BTreeMap ensures consistent ordering regardless of insertion order.
    pub fn cache_key(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        Hash::hash(self, &mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_config_hash_consistency() {
        use pretty_assertions::assert_eq;

        // Create two identical configs
        let fixture1 = McpConfig {
            mcp_servers: BTreeMap::from([
                (
                    "server1".to_string().into(),
                    McpServerConfig::new_sse("http://localhost:3000"),
                ),
                (
                    "server2".to_string().into(),
                    McpServerConfig::new_stdio("node", vec![], None),
                ),
            ]),
        };

        let fixture2 = McpConfig {
            mcp_servers: BTreeMap::from([
                (
                    "server1".to_string().into(),
                    McpServerConfig::new_sse("http://localhost:3000"),
                ),
                (
                    "server2".to_string().into(),
                    McpServerConfig::new_stdio("node", vec![], None),
                ),
            ]),
        };

        // Hashes should be identical
        let actual = fixture1.cache_key();
        let expected = fixture2.cache_key();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_mcp_config_hash_different_configs() {
        use pretty_assertions::assert_ne;

        // Create two different configs
        let fixture1 = McpConfig {
            mcp_servers: BTreeMap::from([(
                "server1".to_string().into(),
                McpServerConfig::new_sse("http://localhost:3000"),
            )]),
        };

        let fixture2 = McpConfig {
            mcp_servers: BTreeMap::from([(
                "server1".to_string().into(),
                McpServerConfig::new_sse("http://localhost:3001"),
            )]),
        };

        // Hashes should be different
        let actual = fixture1.cache_key();
        let expected = fixture2.cache_key();
        assert_ne!(actual, expected);
    }

    #[test]
    fn test_mcp_config_hash_insertion_order_independent() {
        use pretty_assertions::assert_eq;

        // Create config with servers in one order
        let fixture1 = McpConfig {
            mcp_servers: BTreeMap::from([
                (
                    "a_server".to_string().into(),
                    McpServerConfig::new_sse("http://a"),
                ),
                (
                    "z_server".to_string().into(),
                    McpServerConfig::new_sse("http://z"),
                ),
            ]),
        };

        // Create config with servers in different order (BTreeMap sorts by key)
        let fixture2 = McpConfig {
            mcp_servers: BTreeMap::from([
                (
                    "z_server".to_string().into(),
                    McpServerConfig::new_sse("http://z"),
                ),
                (
                    "a_server".to_string().into(),
                    McpServerConfig::new_sse("http://a"),
                ),
            ]),
        };

        // Hashes should be identical because BTreeMap maintains sorted order
        let actual = fixture1.cache_key();
        let expected = fixture2.cache_key();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_mcp_server_config_disabled() {
        let server = McpStdioServer { disable: true, ..Default::default() };

        let config = McpServerConfig::Stdio(server);
        assert!(config.is_disabled());

        let sse_server = McpSseServer { disable: false, ..Default::default() };

        let config = McpServerConfig::Sse(sse_server);
        assert!(!config.is_disabled());
    }

    #[test]
    fn test_mcp_config_deserialization_valid() {
        use pretty_assertions::assert_eq;

        let json = r#"{
            "mcpServers": {
                "test_server": {
                    "command": "node",
                    "args": ["server.js"]
                }
            }
        }"#;

        let actual: McpConfig = serde_json::from_str(json).unwrap();
        let expected = McpConfig {
            mcp_servers: BTreeMap::from([(
                "test_server".to_string().into(),
                McpServerConfig::new_stdio("node", vec!["server.js".to_string()], None),
            )]),
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_mcp_config_deserialization_empty_object() {
        let json = "{}";
        let result = serde_json::from_str::<McpConfig>(json);

        assert!(result.is_ok());
    }

    #[test]
    fn test_mcp_config_deserialization_wrong_field_name() {
        let json = r#"{"servers": {"test": {}}}"#;
        let result = serde_json::from_str::<McpConfig>(json);

        assert!(result.is_err());
    }

    #[test]
    fn test_mcp_config_deserialization_null_mcp_servers() {
        let json = r#"{"mcpServers": null}"#;
        let result = serde_json::from_str::<McpConfig>(json);

        assert!(result.is_err());
    }

    #[test]
    fn test_mcp_sse_server_with_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());

        let server = McpSseServer {
            url: "https://api.example.com".to_string(),
            headers,
            disable: false,
        };

        let config = McpServerConfig::Sse(server);
        assert!(!config.is_disabled());
    }

    #[test]
    fn test_mcp_sse_server_new_sse_with_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());

        let config =
            McpServerConfig::new_sse_with_headers("https://api.example.com", headers.clone());

        match config {
            McpServerConfig::Sse(sse) => {
                assert_eq!(sse.url, "https://api.example.com");
                assert_eq!(sse.headers, headers);
                assert!(!sse.disable);
            }
            _ => panic!("Expected SSE config"),
        }
    }

    #[test]
    fn test_mcp_config_with_headers_deserialization() {
        use pretty_assertions::assert_eq;

        let json = r#"{
            "mcpServers": {
                "test_server": {
                    "url": "https://api.example.com",
                    "headers": {
                        "Authorization": "Bearer token123"
                    }
                }
            }
        }"#;

        let result: McpConfig = serde_json::from_str(json).unwrap();
        match result.mcp_servers.get(&"test_server".to_string().into()) {
            Some(McpServerConfig::Sse(sse)) => {
                assert_eq!(sse.url, "https://api.example.com");
                assert_eq!(
                    sse.headers.get("Authorization"),
                    Some(&"Bearer token123".to_string())
                );
            }
            _ => panic!("Expected SSE config"),
        }
    }

    #[test]
    fn test_mcp_sse_server_display_with_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());
        headers.insert("X-Custom".to_string(), "value".to_string());

        let server = McpSseServer {
            url: "https://api.example.com".to_string(),
            headers,
            disable: false,
        };

        let config = McpServerConfig::Sse(server);
        let display = format!("{}", config);

        assert!(display.contains("https://api.example.com"));
        assert!(display.contains("Authorization:Bearer token123"));
        assert!(display.contains("X-Custom:value"));
    }
}
