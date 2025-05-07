use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Represents the type-safe data for different tool responses
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum ToolResponseData {
    /// Response from file read operations
    FileRead {
        /// Path of the file that was read
        path: String,
        /// Total characters in the file
        total_chars: Option<usize>,
        /// Range of characters that were read (start, end)
        char_range: Option<(usize, usize)>,
        /// Whether the file is binary
        is_binary: Option<bool>,
    },
    /// Response from shell command execution
    Shell {
        /// The command that was executed
        command: String,
        /// Exit code of the command
        exit_code: Option<i32>,
        /// Total characters in the output
        total_chars: Option<usize>,
        /// Whether the output was truncated
        truncated: Option<bool>,
    },
    /// Response from file write operations
    FileWrite {
        /// Path of the file that was written
        path: String,
        /// Number of bytes written
        bytes_written: usize,
        /// Whether the file was created
        created: Option<bool>,
    },
    /// Response from web search operations
    WebSearch {
        /// The query that was searched
        query: String,
        /// Number of results returned
        result_count: usize,
    },
    /// Response from web fetch operations
    WebFetch {
        /// The URL that was fetched
        url: String,
        /// HTTP status code
        status_code: Option<u16>,
    },
    /// Response from codebase retrieval operations
    CodebaseRetrieval {
        /// The query that was used for retrieval
        query: String,
        /// Number of results returned
        result_count: Option<usize>,
    },
    /// Generic response for tools that don't have a specific type
    Generic {
        /// Additional metadata as key-value pairs
        metadata: HashMap<String, Value>,
    },
}

impl Default for ToolResponseData {
    fn default() -> Self {
        Self::Generic {
            metadata: HashMap::new(),
        }
    }
}

impl ToolResponseData {
    /// Converts the tool response data to a front matter format
    pub fn to_front_matter(&self, content: &str) -> String {
        let metadata = match self {
            ToolResponseData::FileRead { path, total_chars, char_range, is_binary } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "file_read".into());
                map.insert("path".to_string(), path.clone().into());

                if let Some(tc) = total_chars {
                    map.insert("total_chars".to_string(), (*tc).into());
                }

                if let Some((start, end)) = char_range {
                    map.insert("char_range".to_string(), format!("{}-{}", start, end).into());
                }

                if let Some(binary) = is_binary {
                    map.insert("is_binary".to_string(), (*binary).into());
                }

                map
            },
            ToolResponseData::Shell { command, exit_code, total_chars, truncated } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "shell".into());
                map.insert("command".to_string(), command.clone().into());

                if let Some(code) = exit_code {
                    map.insert("exit_code".to_string(), (*code).into());
                }

                if let Some(tc) = total_chars {
                    map.insert("total_chars".to_string(), (*tc).into());
                }

                if let Some(t) = truncated {
                    map.insert("truncated".to_string(), (*t).into());
                }

                map
            },
            ToolResponseData::FileWrite { path, bytes_written, created } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "file_write".into());
                map.insert("path".to_string(), path.clone().into());
                map.insert("bytes_written".to_string(), (*bytes_written).into());

                if let Some(c) = created {
                    map.insert("created".to_string(), (*c).into());
                }

                map
            },
            ToolResponseData::WebSearch { query, result_count } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "web_search".into());
                map.insert("query".to_string(), query.clone().into());
                map.insert("result_count".to_string(), (*result_count).into());
                map
            },
            ToolResponseData::WebFetch { url, status_code } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "web_fetch".into());
                map.insert("url".to_string(), url.clone().into());

                if let Some(code) = status_code {
                    map.insert("status_code".to_string(), (*code).into());
                }

                map
            },
            ToolResponseData::CodebaseRetrieval { query, result_count } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "codebase_retrieval".into());
                map.insert("query".to_string(), query.clone().into());

                if let Some(count) = result_count {
                    map.insert("result_count".to_string(), (*count).into());
                }

                map
            },
            ToolResponseData::Generic { metadata } => {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "generic".into());

                for (key, value) in metadata {
                    map.insert(key.clone(), value.clone());
                }

                map
            },
        };

        // Create a sorted list of keys to ensure consistent order
        let mut keys: Vec<&String> = metadata.keys().collect();
        keys.sort();

        // Build the YAML string manually to ensure consistent order
        let mut yaml_string = String::new();

        // Always put type first if it exists
        if let Some(type_value) = metadata.get("type") {
            if let Some(type_str) = type_value.as_str() {
                yaml_string.push_str(&format!("type: {}\n", type_str));
            }
        }

        // Always put tool_name second if it exists
        if let Some(tool_name_value) = metadata.get("tool_name") {
            if let Some(tool_name_str) = tool_name_value.as_str() {
                yaml_string.push_str(&format!("tool_name: {}\n", tool_name_str));
            }
        }

        // Always put status third if it exists
        if let Some(status_value) = metadata.get("status") {
            if let Some(status_str) = status_value.as_str() {
                yaml_string.push_str(&format!("status: {}\n", status_str));
            }
        }

        // Then add all other keys in alphabetical order
        for key in keys {
            if key != "type" && key != "tool_name" && key != "status" {
                let value = &metadata[key];

                // Handle different value types
                if let Some(str_val) = value.as_str() {
                    // Don't quote simple strings to match the expected format in tests
                    if str_val.contains(' ') || str_val.contains(':') || str_val.contains('"') || str_val.contains('\'') {
                        yaml_string.push_str(&format!("{}: '{}'\n", key, str_val));
                    } else {
                        yaml_string.push_str(&format!("{}: {}\n", key, str_val));
                    }
                } else if value.is_number() || value.is_boolean() {
                    yaml_string.push_str(&format!("{}: {}\n", key, value));
                } else {
                    // For complex types, use serde_yaml for that specific value
                    if let Ok(val_str) = serde_yaml::to_string(value) {
                        let val_str = val_str.trim();
                        yaml_string.push_str(&format!("{}: {}\n", key, val_str));
                    }
                }
            }
        }

        // Format as front matter
        format!("---\n{}---\n{}", yaml_string, content)
    }

    /// Creates a generic tool response data with the given metadata
    pub fn generic(metadata: HashMap<String, Value>) -> Self {
        Self::Generic { metadata }
    }
}
