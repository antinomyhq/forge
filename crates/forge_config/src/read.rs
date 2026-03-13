use std::path::{Path, PathBuf};

use config::{ConfigBuilder, Environment, File, FileFormat, builder::AsyncState};
use serde::de::DeserializeOwned;

use crate::error::Error;

/// Embedded default configuration, compiled into the binary.
const DEFAULT_CONFIG: &str = include_str!("config.yaml");

/// Loads all `.env` files found from the filesystem root down to `cwd`, giving
/// priority to files closer to `cwd` (they are loaded last and thus win on
/// conflicts). Already-set environment variables are never overwritten.
fn load_dot_env(cwd: &Path) {
    let mut paths = vec![];
    let mut current = PathBuf::new();

    for component in cwd.components() {
        current.push(component);
        paths.push(current.clone());
    }

    // Reverse so that the root is loaded first and the closest directory last,
    // giving higher priority to `.env` files nearer to the working directory.
    paths.reverse();

    for path in paths {
        let env_file = path.join(".env");
        if env_file.is_file() {
            dotenvy::from_path(&env_file).ok();
        }
    }
}

/// Reads and deserializes any `T: DeserializeOwned` from the following sources, in increasing
/// priority order:
///
/// 1. `.env` files loaded from the current working directory upward
/// 2. Embedded `default.yaml` (compiled into the binary)
/// 3. A YAML file at `~/forge/forge.yaml` (optional — skipped if the file does not exist)
/// 4. A YAML file at `<cwd>/.forge/forge.yaml` (optional — skipped if the file does not exist)
/// 5. Environment variables (always active)
///
/// CWD-level config files take precedence over home-level ones.
pub async fn read<T: DeserializeOwned>() -> Result<T, Error> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    load_dot_env(&cwd);

    let home_base = dirs::home_dir()
        .map(|a| a.join("forge"))
        .unwrap_or(PathBuf::from(".").join("forge"));
    let home_path = home_base.join("forge");
    let home = home_path.to_string_lossy();

    let project_path = cwd.join(".forge").join("forge");
    let project = project_path.to_string_lossy();

    let cfg = ConfigBuilder::<AsyncState>::default()
        .add_source(File::from_str(DEFAULT_CONFIG, FileFormat::Yaml))
        // Home-level config (lower priority)
        .add_source(File::new(&format!("{home}.yaml"), FileFormat::Yaml).required(false))
        // Project-level config (higher priority, overrides home)
        .add_source(File::new(&format!("{project}.yaml"), FileFormat::Yaml).required(false))
        .add_source(
            Environment::with_prefix("FORGE")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true)
                .list_separator(",")
                .with_list_parse_key("retry_status_codes"),
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
        let result = read().await;
        unsafe { std::env::remove_var(key) };

        result
    }

    #[tokio::test]
    async fn test_deeply_nested_env_var() {
        let config = read_env("FORGE_HTTP_HICKORY=true").await.unwrap();
        let actual = config.http_hickory;
        let expected = Some(true);

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_deeply_nested_env_var_with_underscore_field() {
        let config = read_env("FORGE_HTTP_CONNECT_TIMEOUT=42").await.unwrap();
        let actual = config.http_connect_timeout;
        let expected = Some(42u64);

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_http_keep_alive_interval() {
        let config = read_env("FORGE_HTTP_KEEP_ALIVE_INTERVAL=30").await.unwrap();
        let actual = config.http_keep_alive_interval;
        let expected = Some(30u64);

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_retry_status_codes() {
        let config = read_env("FORGE_RETRY_STATUS_CODES=429,500,502")
            .await
            .unwrap();
        let actual = config.retry_status_codes.clone();
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
        let config = read_env("FORGE_CURRENCY_CONVERSION_RATE=1.5")
            .await
            .unwrap();
        let actual = config.currency_conversion_rate;
        let expected = Some(1.5f64);

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_compaction_config() {
        let config = read_env("FORGE_COMPACTION__TURN_THRESHOLD=10")
            .await
            .unwrap();
        let actual = config.compaction.unwrap().turn_threshold;
        let expected = Some(10usize);

        assert_eq!(actual, expected);
    }
}
