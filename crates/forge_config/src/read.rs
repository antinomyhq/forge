use config::{ConfigBuilder, Environment, File, FileFormat, builder::AsyncState};
use serde::de::DeserializeOwned;

use crate::error::Error;

/// Embedded default configuration, compiled into the binary.
const DEFAULT_CONFIG: &str = include_str!("config.yaml");

/// Reads and deserializes any `T: DeserializeOwned` from the following sources, in increasing
/// priority order:
///
/// 1. Embedded `default.yaml` (compiled into the binary)
/// 2. A YAML file at `{path}.yaml` (optional — skipped if the file does not exist)
/// 3. A JSON file at `{path}.json` (optional — skipped if the file does not exist)
/// 4. Environment variables (always active)
///
/// # Errors
///
/// Returns [`Error`] if any source fails to parse or if deserialization into `T` fails.
pub async fn read<T: DeserializeOwned>(path: &str) -> Result<T, Error> {
    let cfg = ConfigBuilder::<AsyncState>::default()
        .add_source(File::from_str(DEFAULT_CONFIG, FileFormat::Yaml))
        .add_source(File::new(&format!("{path}.yaml"), FileFormat::Yaml).required(false))
        .add_source(File::new(&format!("{path}.json"), FileFormat::Json).required(false))
        .add_source(
            Environment::with_prefix("FORGE")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true)
                .list_separator(",")
                .with_list_parse_key("retry.retry_status_codes"),
        )
        .build()
        .await?;

    Ok(cfg.try_deserialize::<T>()?)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::config::ForgeConfig;

    use super::*;

    async fn read_env(env_var: &str) -> Result<ForgeConfig, Error> {
        let (key, value) = env_var
            .split_once('=')
            .expect("env_var must be in KEY=VALUE format");

        // SAFETY: tests using this helper run on tokio's single-threaded runtime;
        // no other thread reads or writes this variable concurrently.
        unsafe { std::env::set_var(key, value) };
        let result = read("").await;
        unsafe { std::env::remove_var(key) };

        result
    }

    #[tokio::test]
    async fn test_deeply_nested_env_var() {
        let config = read_env("FORGE_HTTP__HICKORY=true").await.unwrap();
        let actual = config.http.as_ref().and_then(|h| h.hickory);
        let expected = Some(true);

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_deeply_nested_env_var_with_underscore_field() {
        let config = read_env("FORGE_HTTP__CONNECT_TIMEOUT=42").await.unwrap();
        let actual = config.http.as_ref().and_then(|h| h.connect_timeout);
        let expected = Some(42u64);

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_http_keep_alive_interval() {
        let config = read_env("FORGE_HTTP__KEEP_ALIVE_INTERVAL=30").await.unwrap();
        let actual = config.http.as_ref().and_then(|h| h.keep_alive_interval);
        let expected = Some(30u64);

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_retry_status_codes() {
        let config = read_env("FORGE_RETRY__RETRY_STATUS_CODES=429,500,502")
            .await
            .unwrap();
        let actual = config
            .retry
            .as_ref()
            .and_then(|r| r.retry_status_codes.clone());
        let expected = Some(vec![429u16, 500u16, 502u16]);

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_banner() {
        let config = read_env("FORGE_BANNER=hello").await.unwrap();
        let actual = config.banner.clone();
        let expected = Some("hello".to_string());

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_currency_symbol() {
        let config = read_env("FORGE_CURRENCY_SYMBOL=$").await.unwrap();
        let actual = config.currency_symbol.clone();
        let expected = Some("$".to_string());

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_currency_conversion_rate() {
        let config = read_env("FORGE_CURRENCY_CONVERSION_RATE=1.5").await.unwrap();
        let actual = config.currency_conversion_rate;
        let expected = Some(1.5f64);

        assert_eq!(actual, expected);
    }
}
