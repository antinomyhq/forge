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
    Http(McpHttpServer),
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

    /// Create a new HTTP-based MCP server (auto-detects transport type)
    pub fn new_http(url: impl Into<String>) -> Self {
        Self::Http(McpHttpServer { url: url.into(), headers: BTreeMap::new(), disable: false })
    }

    pub fn is_disabled(&self) -> bool {
        match self {
            McpServerConfig::Stdio(v) => v.disable,
            McpServerConfig::Http(v) => v.disable,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Setters, PartialEq, Hash)]
#[setters(strip_option, into)]
pub struct McpStdioServer {
    /// Command to execute for starting this MCP server
    #[serde(skip_serializing_if = "String::is_empty")]
    pub command: String,

    /// Arguments to pass to the command
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables to pass to the command
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,

    /// Disable it temporarily without having to
    /// remove it from the config.
    #[serde(default)]
    pub disable: bool,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct McpHttpServer {
    /// Url of the MCP server (auto-detects HTTP vs SSE transport)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,

    /// Optional headers for HTTP requests
    /// Supports mustache templates for environment variables: {{.env.VAR_NAME}}
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,

    /// Disable it temporarily without having to
    /// remove it from the config.
    #[serde(default)]
    pub disable: bool,
}

impl McpHttpServer {
    /// Resolves mustache templates in the entire config using provided
    /// environment variables
    pub fn resolve(self, env_vars: &BTreeMap<String, String>) -> Self {
        Self {
            url: self.url,
            headers: self
                .headers
                .into_iter()
                .map(|(key, value)| {
                    let resolved_value = Self::resolve_template(&value, env_vars);
                    (key, resolved_value)
                })
                .collect(),
            disable: self.disable,
        }
    }

    fn resolve_template(template: &str, env_vars: &BTreeMap<String, String>) -> String {
        // Simple regex-based mustache template resolution for {{.env.VAR_NAME}}
        let re = regex::Regex::new(r"\{\{\.env\.([A-Z_][A-Z0-9_]*)\}\}").unwrap();

        re.replace_all(template, |caps: &regex::Captures| {
            let var_name = &caps[1];
            env_vars
                .get(var_name)
                .map(|s| s.to_string())
                .unwrap_or_else(|| caps[0].to_string())
        })
        .to_string()
    }
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
            McpServerConfig::Http(sse) => {
                output.push_str(&format!("{} ", sse.url));
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
                    McpServerConfig::new_http("http://localhost:3000"),
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
                    McpServerConfig::new_http("http://localhost:3000"),
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
                McpServerConfig::new_http("http://localhost:3000"),
            )]),
        };

        let fixture2 = McpConfig {
            mcp_servers: BTreeMap::from([(
                "server1".to_string().into(),
                McpServerConfig::new_http("http://localhost:3001"),
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
                    McpServerConfig::new_http("http://a"),
                ),
                (
                    "z_server".to_string().into(),
                    McpServerConfig::new_http("http://z"),
                ),
            ]),
        };

        // Create config with servers in different order (BTreeMap sorts by key)
        let fixture2 = McpConfig {
            mcp_servers: BTreeMap::from([
                (
                    "z_server".to_string().into(),
                    McpServerConfig::new_http("http://z"),
                ),
                (
                    "a_server".to_string().into(),
                    McpServerConfig::new_http("http://a"),
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

        let sse_server = McpHttpServer { disable: false, ..Default::default() };

        let config = McpServerConfig::Http(sse_server);
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
    fn test_http_server_with_headers() {
        use pretty_assertions::assert_eq;

        let json = r#"{
            "mcpServers": {
                "github": {
                    "url": "https://api.githubcopilot.com/mcp/",
                    "headers": {
                        "Authorization": "Bearer test_token",
                        "Content-Type": "application/json"
                    }
                }
            }
        }"#;

        let actual: McpConfig = serde_json::from_str(json).unwrap();

        match actual.mcp_servers.get(&"github".to_string().into()) {
            Some(McpServerConfig::Http(server)) => {
                assert_eq!(server.url, "https://api.githubcopilot.com/mcp/");
                assert_eq!(server.headers.len(), 2);
                assert_eq!(
                    server.headers.get("Authorization"),
                    Some(&"Bearer test_token".to_string())
                );
            }
            _ => panic!("Expected Http variant"),
        }
    }

    #[test]
    fn test_resolve_with_env_template() {
        use pretty_assertions::assert_eq;

        let env_vars = BTreeMap::from([
            ("GH_TOKEN".to_string(), "secret_token_123".to_string()),
            ("API_KEY".to_string(), "api_key_456".to_string()),
        ]);

        let server = McpHttpServer {
            url: "https://api.example.com".to_string(),
            headers: BTreeMap::from([
                (
                    "Authorization".to_string(),
                    "Bearer {{.env.GH_TOKEN}}".to_string(),
                ),
                ("X-API-Key".to_string(), "{{.env.API_KEY}}".to_string()),
                ("Content-Type".to_string(), "application/json".to_string()),
            ]),
            disable: false,
        };

        let resolved = server.resolve(&env_vars);

        assert_eq!(
            resolved.headers.get("Authorization"),
            Some(&"Bearer secret_token_123".to_string())
        );
        assert_eq!(
            resolved.headers.get("X-API-Key"),
            Some(&"api_key_456".to_string())
        );
        assert_eq!(
            resolved.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_resolve_missing_env_var() {
        use pretty_assertions::assert_eq;

        let env_vars = BTreeMap::new(); // Empty env vars

        let server = McpHttpServer {
            url: "https://api.example.com".to_string(),
            headers: BTreeMap::from([(
                "Authorization".to_string(),
                "Bearer {{.env.MISSING_VAR}}".to_string(),
            )]),
            disable: false,
        };

        let resolved = server.resolve(&env_vars);

        // Should keep the template if env var is missing
        assert_eq!(
            resolved.headers.get("Authorization"),
            Some(&"Bearer {{.env.MISSING_VAR}}".to_string())
        );
    }
}
