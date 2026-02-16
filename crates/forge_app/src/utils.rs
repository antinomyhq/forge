use std::path::Path;

use futures::stream::{self, StreamExt};

use crate::{Match, MatchResult};

/// Formats a path for display, converting absolute paths to relative when
/// possible
///
/// If the path starts with the current working directory, returns a
/// relative path. Otherwise, returns the original absolute path.
///
/// # Arguments
/// * `path` - The path to format
/// * `cwd` - The current working directory path
///
/// # Returns
/// * A formatted path string
pub fn format_display_path(path: &Path, cwd: &Path) -> String {
    // Try to create a relative path for display if possible
    let display_path = if path.starts_with(cwd) {
        match path.strip_prefix(cwd) {
            Ok(rel_path) => rel_path.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    } else {
        path.display().to_string()
    };

    if display_path.is_empty() {
        ".".to_string()
    } else {
        display_path
    }
}

/// Truncates a key string for display purposes
///
/// If the key length is 20 characters or less, returns it unchanged.
/// Otherwise, shows the first 13 characters and last 4 characters with "..." in
/// between.
///
/// # Arguments
/// * `key` - The key string to truncate
///
/// # Returns
/// * A truncated version of the key for safe display
pub use forge_domain::truncate_key;

pub fn format_match(matched: &Match, base_dir: &Path) -> String {
    match &matched.result {
        Some(MatchResult::Error(err)) => format!("Error reading {}: {}", matched.path, err),
        Some(MatchResult::Found { line_number, line }) => {
            let path = format_display_path(Path::new(&matched.path), base_dir);
            match line_number {
                Some(num) => format!("{}:{}:{}", path, num, line),
                None => format!("{}:{}", path, line),
            }
        }
        Some(MatchResult::Count { count }) => {
            format!(
                "{}:{}",
                format_display_path(Path::new(&matched.path), base_dir),
                count
            )
        }
        Some(MatchResult::FileMatch) => format_display_path(Path::new(&matched.path), base_dir),
        Some(MatchResult::ContextMatch { line_number, line, before_context, after_context }) => {
            let path = format_display_path(Path::new(&matched.path), base_dir);
            let mut output = String::new();

            // Add before context lines
            for ctx_line in before_context {
                output.push_str(&format!("{}-{}\n", path, ctx_line));
            }

            // Add the match line
            match line_number {
                Some(num) => output.push_str(&format!("{}:{}:{}", path, num, line)),
                None => output.push_str(&format!("{}:{}", path, line)),
            }

            // Add after context lines
            for ctx_line in after_context {
                output.push_str(&format!("\n{}-{}", path, ctx_line));
            }

            output
        }
        None => format_display_path(Path::new(&matched.path), base_dir),
    }
}

/// Returns the number of CPU cores available for parallel execution
///
/// Detects the number of available CPU cores using
/// `std::thread::available_parallelism()`. Falls back to 1 if detection fails.
///
/// # Returns
/// * The number of CPU cores, or 1 if detection fails
pub fn get_concurrency_limit() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Executes async operations in parallel with bounded concurrency
///
/// Processes items from an iterator by applying an async function to each item,
/// limiting the number of concurrent operations to the number of CPU cores.
/// This prevents resource exhaustion when processing large collections.
///
/// # Arguments
/// * `items` - Iterator of items to process
/// * `f` - Async function to apply to each item
///
/// # Returns
/// * Vector of results from all operations
///
/// # Example
///
/// ```rust,ignore
/// use forge_app::utils::parallel_map;
///
/// let files = vec!["file1.txt", "file2.txt", "file3.txt"];
/// let results = parallel_map(files, |file| async move {
///     // Read file and compute hash
///     read_and_hash(file).await
/// }).await;
/// ```
pub async fn parallel_map<I, T, F, Fut, R>(items: I, f: F) -> Vec<R>
where
    I: IntoIterator<Item = T>,
    F: Fn(T) -> Fut,
    Fut: std::future::Future<Output = R>,
{
    let concurrency = get_concurrency_limit();

    stream::iter(items)
        .map(f)
        .buffer_unordered(concurrency)
        .collect()
        .await
}

/// Computes SHA-256 hash of the given content
///
/// General-purpose utility function that computes a SHA-256 hash of string
/// content. Returns a consistent hexadecimal representation that can be used
/// for content comparison, caching, or change detection.
///
/// # Arguments
/// * `content` - The content string to hash
///
/// # Returns
/// * A hexadecimal string representation of the SHA-256 hash
pub fn compute_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Normalizes a JSON schema to meet LLM provider requirements
///
/// Many LLM providers (OpenAI, Anthropic) require that all object types in JSON
/// schemas explicitly set `additionalProperties: false`. This function
/// recursively processes the schema to add this requirement.
///
/// Additionally, for OpenAI compatibility, it ensures:
/// - All objects have a `properties` field (even if empty)
/// - All objects have a `required` array with all property keys
///
/// # Arguments
/// * `schema` - The JSON schema to normalize (will be modified in place)
/// * `strict_mode` - If true, adds `properties` and `required` fields for
///   OpenAI compatibility
///
/// # Example
///
/// ```rust,ignore
/// use serde_json::json;
/// use forge_app::utils::normalize_json_schema;
///
/// let mut schema = json!({
///     "type": "object",
///     "properties": {
///         "name": { "type": "string" }
///     }
/// });
///
/// normalize_json_schema(&mut schema, false);
///
/// assert_eq!(schema["additionalProperties"], json!(false));
/// ```
pub fn enforce_strict_schema(schema: &mut serde_json::Value, strict_mode: bool) {
    match schema {
        serde_json::Value::Object(map) => {
            // Check if this is an object type
            let is_object = map
                .get("type")
                .and_then(|value| value.as_str())
                .is_some_and(|ty| ty == "object")
                || map.contains_key("properties");

            if is_object {
                // OpenAI strict mode: ensure properties field exists
                if strict_mode && !map.contains_key("properties") {
                    map.insert(
                        "properties".to_string(),
                        serde_json::Value::Object(serde_json::Map::new()),
                    );
                }

                // Both OpenAI and Anthropic require this field to be `false` for objects
                map.insert(
                    "additionalProperties".to_string(),
                    serde_json::Value::Bool(false),
                );

                // OpenAI strict mode: ensure required field exists with all property keys
                if strict_mode {
                    let required_keys = map
                        .get("properties")
                        .and_then(|value| value.as_object())
                        .map(|props| {
                            let mut keys = props.keys().cloned().collect::<Vec<_>>();
                            keys.sort();
                            keys
                        })
                        .unwrap_or_default();

                    let required_values = required_keys
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect::<Vec<_>>();

                    map.insert(
                        "required".to_string(),
                        serde_json::Value::Array(required_values),
                    );
                }
            }

            // OpenAI strict mode: convert "nullable: true" to anyOf with null type.
            // OpenAI does not support the "nullable" keyword; instead, nullable
            // schemas must be expressed as anyOf: [<original_schema>, {type: "null"}].
            // The description is kept at the top level alongside the anyOf.
            // Additionally, schemars' AddNullable transform adds `null` to enum
            // arrays for nullable enums, which must be stripped from the non-null
            // branch.
            if strict_mode
                && map
                    .get("nullable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            {
                map.remove("nullable");

                // Remove null from enum array if present (added by AddNullable)
                if let Some(serde_json::Value::Array(enum_values)) = map.get_mut("enum") {
                    enum_values.retain(|v| !v.is_null());
                }

                // Extract description to keep at the top level
                let description = map.remove("description");

                // Build the non-null branch from remaining keys
                let non_null_branch = serde_json::Value::Object(std::mem::take(map));
                let null_branch = serde_json::json!({"type": "null"});

                // Replace the current map contents with an anyOf wrapper
                if let Some(desc) = description {
                    map.insert("description".to_string(), desc);
                }
                map.insert(
                    "anyOf".to_string(),
                    serde_json::Value::Array(vec![non_null_branch, null_branch]),
                );
            }

            // Recursively normalize nested schemas
            for value in map.values_mut() {
                enforce_strict_schema(value, strict_mode);
            }
        }
        serde_json::Value::Array(items) => {
            for value in items {
                enforce_strict_schema(value, strict_mode);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_get_concurrency_limit_returns_positive_number() {
        let actual = get_concurrency_limit();
        assert!(actual > 0, "Concurrency limit should be at least 1");
    }

    #[tokio::test]
    async fn test_parallel_map_processes_all_items() {
        let items = vec![1, 2, 3, 4, 5];
        let actual = parallel_map(items, |x| async move { x * 2 }).await;
        let expected = vec![2, 4, 6, 8, 10];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_parallel_map_with_empty_input() {
        let items: Vec<i32> = vec![];
        let actual = parallel_map(items, |x| async move { x * 2 }).await;
        let expected: Vec<i32> = vec![];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_parallel_map_with_async_operations() {
        let items = vec!["hello", "world", "rust"];
        let actual = parallel_map(items, |s| async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
            s.to_uppercase()
        })
        .await;
        let expected = vec!["HELLO", "WORLD", "RUST"];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_normalize_json_schema_anthropic_mode() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        });

        enforce_strict_schema(&mut schema, false);

        assert_eq!(schema["additionalProperties"], json!(false));
        // In non-strict mode, required field is not added
        assert_eq!(schema.get("required"), None);
    }

    #[test]
    fn test_normalize_json_schema_openai_strict_mode() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "number" }
            }
        });

        enforce_strict_schema(&mut schema, true);

        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(schema["required"], json!(["age", "name"]));
    }

    #[test]
    fn test_normalize_json_schema_adds_empty_properties_in_strict_mode() {
        let mut schema = json!({
            "type": "object"
        });

        enforce_strict_schema(&mut schema, true);

        assert_eq!(schema["properties"], json!({}));
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(schema["required"], json!([]));
    }

    #[test]
    fn test_normalize_json_schema_nested_objects() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    }
                }
            }
        });

        enforce_strict_schema(&mut schema, false);

        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(
            schema["properties"]["user"]["additionalProperties"],
            json!(false)
        );
    }

    #[test]
    fn test_nullable_enum_converted_to_any_of_in_strict_mode() {
        // This matches what schemars AddNullable produces: nullable=true AND
        // null added to enum values array
        let mut schema = json!({
            "type": "object",
            "properties": {
                "output_mode": {
                    "description": "Output mode",
                    "nullable": true,
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count", null]
                }
            }
        });

        enforce_strict_schema(&mut schema, true);

        let expected = json!({
            "type": "object",
            "properties": {
                "output_mode": {
                    "description": "Output mode",
                    "anyOf": [
                        { "type": "string", "enum": ["content", "files_with_matches", "count"] },
                        { "type": "null" }
                    ]
                }
            },
            "additionalProperties": false,
            "required": ["output_mode"]
        });

        assert_eq!(schema, expected);
    }

    #[test]
    fn test_nullable_string_converted_to_any_of_in_strict_mode() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "name": {
                    "description": "A name",
                    "nullable": true,
                    "type": "string"
                }
            }
        });

        enforce_strict_schema(&mut schema, true);

        let expected = json!({
            "type": "object",
            "properties": {
                "name": {
                    "description": "A name",
                    "anyOf": [
                        { "type": "string" },
                        { "type": "null" }
                    ]
                }
            },
            "additionalProperties": false,
            "required": ["name"]
        });

        assert_eq!(schema, expected);
    }

    #[test]
    fn test_nullable_not_converted_in_non_strict_mode() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "output_mode": {
                    "nullable": true,
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"]
                }
            }
        });

        enforce_strict_schema(&mut schema, false);

        // In non-strict mode, nullable should be preserved as-is
        assert_eq!(schema["properties"]["output_mode"]["nullable"], json!(true));
        assert!(schema["properties"]["output_mode"].get("anyOf").is_none());
    }
}
