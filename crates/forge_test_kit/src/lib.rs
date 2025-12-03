//! Test utilities and helpers for Forge tests
//!
//! This crate provides common utilities for testing, including fixture loading
//! helpers that reduce boilerplate in test code.

/// Loads a fixture file from the calling crate's directory
///
/// # Arguments
/// * `path` - Path relative to the crate's manifest directory
///
/// # Example
/// ```ignore
/// let content = fixture("src/fixtures/test.json").await;
/// ```
pub async fn fixture(path: &str) -> String {
    tokio::fs::read_to_string(path)
        .await
        .unwrap_or_else(|e| panic!("Failed to load fixture at {path}: {e}"))
}

/// Macro to load a fixture file relative to the calling crate's manifest
/// directory
///
/// # Example
/// ```ignore
/// let content = fixture!("src/fixtures/test.json").await;
/// ```
#[macro_export]
macro_rules! fixture {
    ($path:expr) => {
        $crate::fixture(&format!("{}/{}", env!("CARGO_MANIFEST_DIR"), $path))
    };
}

/// Loads a fixture file and parses it as JSON
///
/// # Example
/// ```ignore
/// let data: MyType = json_fixture("src/fixtures/test.json").await;
/// ```
#[cfg(feature = "json")]
pub async fn json_fixture<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let content = fixture(path).await;
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse JSON fixture at {}: {}", path, e))
}

/// Macro to load and parse a JSON fixture
///
/// # Example
/// ```ignore
/// let data: MyType = json_fixture!("src/fixtures/test.json").await;
/// ```
#[cfg(feature = "json")]
#[macro_export]
macro_rules! json_fixture {
    ($path:expr) => {
        $crate::json_fixture(&format!("{}/{}", env!("CARGO_MANIFEST_DIR"), $path))
    };
}
