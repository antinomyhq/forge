use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_app::EnvironmentInfra;
use forge_config::{ConfigReader, ForgeConfig, ModelConfig};
use forge_domain::{
    AutoDumpFormat, ConfigOperation, Environment, HttpConfig, RetryConfig, SessionConfig,
    TlsBackend, TlsVersion,
};
use reqwest::Url;
use tracing::{debug, error};

/// Converts a [`ModelConfig`] into a domain-level [`SessionConfig`].
fn to_session_config(mc: &ModelConfig) -> SessionConfig {
    SessionConfig {
        provider_id: mc.provider_id.clone(),
        model_id: mc.model_id.clone(),
    }
}

/// Converts a [`forge_config::TlsVersion`] into a [`forge_domain::TlsVersion`].
fn to_tls_version(v: forge_config::TlsVersion) -> TlsVersion {
    match v {
        forge_config::TlsVersion::V1_0 => TlsVersion::V1_0,
        forge_config::TlsVersion::V1_1 => TlsVersion::V1_1,
        forge_config::TlsVersion::V1_2 => TlsVersion::V1_2,
        forge_config::TlsVersion::V1_3 => TlsVersion::V1_3,
    }
}

/// Converts a [`forge_config::TlsBackend`] into a [`forge_domain::TlsBackend`].
fn to_tls_backend(b: forge_config::TlsBackend) -> TlsBackend {
    match b {
        forge_config::TlsBackend::Default => TlsBackend::Default,
        forge_config::TlsBackend::Rustls => TlsBackend::Rustls,
    }
}

/// Converts a [`forge_config::HttpConfig`] into a [`forge_domain::HttpConfig`].
fn to_http_config(h: forge_config::HttpConfig) -> HttpConfig {
    HttpConfig {
        connect_timeout: h.connect_timeout_secs,
        read_timeout: h.read_timeout_secs,
        pool_idle_timeout: h.pool_idle_timeout_secs,
        pool_max_idle_per_host: h.pool_max_idle_per_host,
        max_redirects: h.max_redirects,
        hickory: h.hickory,
        tls_backend: to_tls_backend(h.tls_backend),
        min_tls_version: h.min_tls_version.map(to_tls_version),
        max_tls_version: h.max_tls_version.map(to_tls_version),
        adaptive_window: h.adaptive_window,
        keep_alive_interval: h.keep_alive_interval_secs,
        keep_alive_timeout: h.keep_alive_timeout_secs,
        keep_alive_while_idle: h.keep_alive_while_idle,
        accept_invalid_certs: h.accept_invalid_certs,
        root_cert_paths: h.root_cert_paths,
    }
}

/// Converts a [`forge_config::RetryConfig`] into a
/// [`forge_domain::RetryConfig`].
fn to_retry_config(r: forge_config::RetryConfig) -> RetryConfig {
    RetryConfig {
        initial_backoff_ms: r.initial_backoff_ms,
        min_delay_ms: r.min_delay_ms,
        backoff_factor: r.backoff_factor,
        max_retry_attempts: r.max_attempts,
        retry_status_codes: r.status_codes,
        max_delay: r.max_delay_secs,
        suppress_retry_errors: r.suppress_errors,
    }
}

/// Converts a [`forge_config::AutoDumpFormat`] into a
/// [`forge_domain::AutoDumpFormat`].
fn to_auto_dump_format(f: forge_config::AutoDumpFormat) -> AutoDumpFormat {
    match f {
        forge_config::AutoDumpFormat::Json => AutoDumpFormat::Json,
        forge_config::AutoDumpFormat::Html => AutoDumpFormat::Html,
    }
}

/// Builds a [`forge_domain::Environment`] entirely from a [`ForgeConfig`] and
/// runtime context (`restricted`, `cwd`), mapping every config field to its
/// corresponding environment field.
fn to_environment(fc: ForgeConfig, restricted: bool, cwd: PathBuf) -> Environment {
    Environment {
        // --- Infrastructure-derived fields ---
        os: std::env::consts::OS.to_string(),
        pid: std::process::id(),
        cwd,
        home: dirs::home_dir(),
        shell: if cfg!(target_os = "windows") {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
        },
        base_path: dirs::home_dir()
            .map(|h| h.join("forge"))
            .unwrap_or_else(|| PathBuf::from(".").join("forge")),

        // --- ForgeConfig-mapped fields ---
        retry_config: fc.retry.map(to_retry_config).unwrap_or_default(),
        max_search_lines: fc.max_search_lines,
        max_search_result_bytes: fc.max_search_result_bytes,
        fetch_truncation_limit: fc.max_fetch_chars,
        stdout_max_prefix_length: fc.max_stdout_prefix_lines,
        stdout_max_suffix_length: fc.max_stdout_suffix_lines,
        stdout_max_line_length: fc.max_stdout_line_chars,
        max_line_length: fc.max_line_chars,
        max_read_size: fc.max_read_lines,
        max_file_read_batch_size: fc.max_file_read_batch_size,
        http: fc.http.map(to_http_config).unwrap_or_default(),
        max_file_size: fc.max_file_size_bytes,
        max_image_size: fc.max_image_size_bytes,
        tool_timeout: fc.tool_timeout_secs,
        auto_open_dump: fc.auto_open_dump,
        debug_requests: fc.debug_requests,
        custom_history_path: fc.custom_history_path,
        max_conversations: fc.max_conversations,
        sem_search_limit: fc.max_sem_search_results,
        sem_search_top_k: fc.sem_search_top_k,
        service_url: Url::parse(fc.services_url.as_str())
            .unwrap_or_else(|_| Url::parse("http://api.forgecode.dev").unwrap()),
        max_extensions: fc.max_extensions,
        auto_dump: fc.auto_dump.map(to_auto_dump_format),
        parallel_file_reads: fc.max_parallel_file_reads,
        model_cache_ttl: fc.model_cache_ttl_secs,
        session: fc.session.as_ref().map(to_session_config),
        commit: fc.commit.as_ref().map(to_session_config),
        suggest: fc.suggest.as_ref().map(to_session_config),
        is_restricted: restricted,
    }
}

/// Converts a [`forge_domain::RetryConfig`] back into a
/// [`forge_config::RetryConfig`].
fn from_retry_config(r: &RetryConfig) -> forge_config::RetryConfig {
    forge_config::RetryConfig {
        initial_backoff_ms: r.initial_backoff_ms,
        min_delay_ms: r.min_delay_ms,
        backoff_factor: r.backoff_factor,
        max_attempts: r.max_retry_attempts,
        status_codes: r.retry_status_codes.clone(),
        max_delay_secs: r.max_delay,
        suppress_errors: r.suppress_retry_errors,
    }
}

/// Converts a [`forge_domain::HttpConfig`] back into a
/// [`forge_config::HttpConfig`].
fn from_http_config(h: &HttpConfig) -> forge_config::HttpConfig {
    forge_config::HttpConfig {
        connect_timeout_secs: h.connect_timeout,
        read_timeout_secs: h.read_timeout,
        pool_idle_timeout_secs: h.pool_idle_timeout,
        pool_max_idle_per_host: h.pool_max_idle_per_host,
        max_redirects: h.max_redirects,
        hickory: h.hickory,
        tls_backend: from_tls_backend(h.tls_backend.clone()),
        min_tls_version: h.min_tls_version.clone().map(from_tls_version),
        max_tls_version: h.max_tls_version.clone().map(from_tls_version),
        adaptive_window: h.adaptive_window,
        keep_alive_interval_secs: h.keep_alive_interval,
        keep_alive_timeout_secs: h.keep_alive_timeout,
        keep_alive_while_idle: h.keep_alive_while_idle,
        accept_invalid_certs: h.accept_invalid_certs,
        root_cert_paths: h.root_cert_paths.clone(),
    }
}

/// Converts a [`forge_domain::TlsVersion`] back into a
/// [`forge_config::TlsVersion`].
fn from_tls_version(v: TlsVersion) -> forge_config::TlsVersion {
    match v {
        TlsVersion::V1_0 => forge_config::TlsVersion::V1_0,
        TlsVersion::V1_1 => forge_config::TlsVersion::V1_1,
        TlsVersion::V1_2 => forge_config::TlsVersion::V1_2,
        TlsVersion::V1_3 => forge_config::TlsVersion::V1_3,
    }
}

/// Converts a [`forge_domain::TlsBackend`] back into a
/// [`forge_config::TlsBackend`].
fn from_tls_backend(b: TlsBackend) -> forge_config::TlsBackend {
    match b {
        TlsBackend::Default => forge_config::TlsBackend::Default,
        TlsBackend::Rustls => forge_config::TlsBackend::Rustls,
    }
}

/// Converts a [`forge_domain::AutoDumpFormat`] back into a
/// [`forge_config::AutoDumpFormat`].
fn from_auto_dump_format(f: &AutoDumpFormat) -> forge_config::AutoDumpFormat {
    match f {
        AutoDumpFormat::Json => forge_config::AutoDumpFormat::Json,
        AutoDumpFormat::Html => forge_config::AutoDumpFormat::Html,
    }
}

/// Converts an [`Environment`] back into a [`ForgeConfig`] suitable for
/// persisting.
///
/// Builds a fresh [`ForgeConfig`] from [`ForgeConfig::default()`] and maps
/// every field that originated from [`ForgeConfig`] back from the
/// [`Environment`], preserving the round-trip identity. Fields that only exist
/// in [`ForgeConfig`] but are not represented in [`Environment`] (e.g.
/// `updates`, `temperature`, `compact`) remain at their default values.
fn to_forge_config(env: &Environment) -> ForgeConfig {
    let mut fc = ForgeConfig::default();

    // --- Fields mapped through Environment ---
    let default_retry = RetryConfig::default();
    fc.retry = if env.retry_config == default_retry {
        None
    } else {
        Some(from_retry_config(&env.retry_config))
    };
    fc.max_search_lines = env.max_search_lines;
    fc.max_search_result_bytes = env.max_search_result_bytes;
    fc.max_fetch_chars = env.fetch_truncation_limit;
    fc.max_stdout_prefix_lines = env.stdout_max_prefix_length;
    fc.max_stdout_suffix_lines = env.stdout_max_suffix_length;
    fc.max_stdout_line_chars = env.stdout_max_line_length;
    fc.max_line_chars = env.max_line_length;
    fc.max_read_lines = env.max_read_size;
    fc.max_file_read_batch_size = env.max_file_read_batch_size;
    let default_http = HttpConfig::default();
    fc.http = if env.http == default_http {
        None
    } else {
        Some(from_http_config(&env.http))
    };
    fc.max_file_size_bytes = env.max_file_size;
    fc.max_image_size_bytes = env.max_image_size;
    fc.tool_timeout_secs = env.tool_timeout;
    fc.auto_open_dump = env.auto_open_dump;
    fc.debug_requests = env.debug_requests.clone();
    fc.custom_history_path = env.custom_history_path.clone();
    fc.max_conversations = env.max_conversations;
    fc.max_sem_search_results = env.sem_search_limit;
    fc.sem_search_top_k = env.sem_search_top_k;
    fc.services_url = env.service_url.to_string();
    fc.max_extensions = env.max_extensions;
    fc.auto_dump = env.auto_dump.as_ref().map(from_auto_dump_format);
    fc.max_parallel_file_reads = env.parallel_file_reads;
    fc.model_cache_ttl_secs = env.model_cache_ttl;

    // --- Session configs ---
    fc.session = env.session.as_ref().map(|sc| ModelConfig {
        provider_id: sc.provider_id.clone(),
        model_id: sc.model_id.clone(),
    });
    fc.commit = env.commit.as_ref().map(|sc| ModelConfig {
        provider_id: sc.provider_id.clone(),
        model_id: sc.model_id.clone(),
    });
    fc.suggest = env.suggest.as_ref().map(|sc| ModelConfig {
        provider_id: sc.provider_id.clone(),
        model_id: sc.model_id.clone(),
    });
    fc
}

/// Trait for parsing environment variable values with custom logic for
/// different types.
#[cfg(test)]
trait FromEnvStr: Sized {
    fn from_env_str(s: &str) -> Option<Self>;
}

/// Custom implementation for bool with support for multiple truthy values.
/// Supports: "true", "1", "yes" (case-insensitive) as true; everything else as
/// false.
#[cfg(test)]
impl FromEnvStr for bool {
    fn from_env_str(s: &str) -> Option<Self> {
        Some(matches!(s.to_lowercase().as_str(), "true" | "1" | "yes"))
    }
}

// Macro to implement FromEnvStr for types that already implement FromStr
macro_rules! impl_from_env_str_via_from_str {
    ($($t:ty),* $(,)?) => {
        $(
            #[cfg(test)]
            impl FromEnvStr for $t {
                fn from_env_str(s: &str) -> Option<Self> {
                    <$t as std::str::FromStr>::from_str(s).ok()
                }
            }
        )*
    };
}

// Implement FromEnvStr for commonly used types
impl_from_env_str_via_from_str! {
    u8, u16, u32, u64, u128, usize,
    i8, i16, i32, i64, i128, isize,
    f32, f64,
    String,
    forge_domain::TlsBackend,
    forge_domain::TlsVersion,
    forge_domain::AutoDumpFormat,
}

/// Parse environment variable using custom FromEnvStr trait.
#[cfg(test)]
fn parse_env<T: FromEnvStr>(name: &str) -> Option<T> {
    std::env::var(name)
        .ok()
        .and_then(|val| T::from_env_str(&val))
}

/// Infrastructure implementation for managing application configuration with
/// caching support.
///
/// Uses [`ForgeConfig::read`] and [`ForgeConfig::write`] for all file I/O and
/// maintains an in-memory cache to reduce disk access. Also handles
/// environment variable discovery via `.env` files and OS APIs.
pub struct ForgeEnvironmentInfra {
    restricted: bool,
    cwd: PathBuf,
    cache: Arc<std::sync::Mutex<Option<ForgeConfig>>>,
}

impl ForgeEnvironmentInfra {
    /// Creates a new [`ForgeConfigInfra`].
    ///
    /// # Arguments
    /// * `restricted` - If true, enables restricted mode
    /// * `cwd` - The working directory path; used to resolve `.env` files
    pub fn new(restricted: bool, cwd: PathBuf) -> Self {
        Self::dot_env(&cwd);
        Self {
            restricted,
            cwd,
            cache: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Reads [`ForgeConfig`] from disk via [`ForgeConfig::read`].
    fn read_from_disk() -> ForgeConfig {
        match ForgeConfig::read() {
            Ok(config) => {
                debug!(config = ?config, "read .forge.toml");
                config
            }
            Err(e) => {
                // NOTE: This should never-happen
                error!(error = ?e, "Failed to read config file. Using default config.");
                Default::default()
            }
        }
    }

    /// Loads all `.env` files walking up from `cwd`, giving priority to closer
    /// (deeper) files.
    fn dot_env(cwd: &Path) -> Option<()> {
        let mut paths = vec![];
        let mut current = PathBuf::new();

        for component in cwd.components() {
            current.push(component);
            paths.push(current.clone());
        }

        paths.reverse();

        for path in paths {
            let env_file = path.join(".env");
            if env_file.is_file() {
                dotenvy::from_path(&env_file).ok();
            }
        }

        Some(())
    }
}

impl EnvironmentInfra for ForgeEnvironmentInfra {
    fn get_env_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn get_env_vars(&self) -> BTreeMap<String, String> {
        std::env::vars().collect()
    }

    fn get_environment(&self) -> Environment {
        let fc = {
            let mut cache = self.cache.lock().expect("cache mutex poisoned");
            if let Some(ref config) = *cache {
                config.clone()
            } else {
                let config = Self::read_from_disk();
                *cache = Some(config.clone());
                config
            }
        };

        to_environment(fc, self.restricted, self.cwd.clone())
    }

    async fn update_environment(&self, ops: Vec<ConfigOperation>) -> anyhow::Result<()> {
        // Load the global config
        let fc = ConfigReader::default()
            .read_defaults()
            .read_global()
            .build()?;

        debug!(config = ?fc, "loaded config for update");

        // Convert to Environment and apply each operation
        debug!(?ops, "applying app config operations");
        let mut env = to_environment(fc.clone(), self.restricted, self.cwd.clone());
        for op in ops {
            env.apply_op(op);
        }

        // Convert Environment back to ForgeConfig and persist
        let fc = to_forge_config(&env);
        fc.write()?;
        debug!(config = ?fc, "written .forge.toml");

        // Reset cache
        *self.cache.lock().expect("cache mutex poisoned") = None;

        Ok(())
    }
}

/// Resolves retry configuration from environment variables or returns defaults.
#[cfg(test)]
fn resolve_retry_config() -> RetryConfig {
    let mut config = RetryConfig::default();

    if let Some(parsed) = parse_env::<u64>("FORGE_RETRY_INITIAL_BACKOFF_MS") {
        config.initial_backoff_ms = parsed;
    }
    if let Some(parsed) = parse_env::<u64>("FORGE_RETRY_BACKOFF_FACTOR") {
        config.backoff_factor = parsed;
    }
    if let Some(parsed) = parse_env::<usize>("FORGE_RETRY_MAX_ATTEMPTS") {
        config.max_retry_attempts = parsed;
    }
    if let Some(parsed) = parse_env::<bool>("FORGE_SUPPRESS_RETRY_ERRORS") {
        config.suppress_retry_errors = parsed;
    }

    // Special handling for comma-separated status codes
    if let Ok(val) = std::env::var("FORGE_RETRY_STATUS_CODES") {
        let status_codes: Vec<u16> = val
            .split(',')
            .filter_map(|code| code.trim().parse::<u16>().ok())
            .collect();
        if !status_codes.is_empty() {
            config.retry_status_codes = status_codes;
        }
    }

    config
}

#[cfg(test)]
fn resolve_http_config() -> forge_domain::HttpConfig {
    let mut config = forge_domain::HttpConfig::default();

    if let Some(parsed) = parse_env::<u64>("FORGE_HTTP_CONNECT_TIMEOUT") {
        config.connect_timeout = parsed;
    }
    if let Some(parsed) = parse_env::<u64>("FORGE_HTTP_READ_TIMEOUT") {
        config.read_timeout = parsed;
    }
    if let Some(parsed) = parse_env::<u64>("FORGE_HTTP_POOL_IDLE_TIMEOUT") {
        config.pool_idle_timeout = parsed;
    }
    if let Some(parsed) = parse_env::<usize>("FORGE_HTTP_POOL_MAX_IDLE_PER_HOST") {
        config.pool_max_idle_per_host = parsed;
    }
    if let Some(parsed) = parse_env::<usize>("FORGE_HTTP_MAX_REDIRECTS") {
        config.max_redirects = parsed;
    }
    if let Some(parsed) = parse_env::<bool>("FORGE_HTTP_USE_HICKORY") {
        config.hickory = parsed;
    }
    if let Some(parsed) = parse_env::<TlsBackend>("FORGE_HTTP_TLS_BACKEND") {
        config.tls_backend = parsed;
    }
    if let Some(parsed) = parse_env::<TlsVersion>("FORGE_HTTP_MIN_TLS_VERSION") {
        config.min_tls_version = Some(parsed);
    }
    if let Some(parsed) = parse_env::<TlsVersion>("FORGE_HTTP_MAX_TLS_VERSION") {
        config.max_tls_version = Some(parsed);
    }
    if let Some(parsed) = parse_env::<bool>("FORGE_HTTP_ADAPTIVE_WINDOW") {
        config.adaptive_window = parsed;
    }

    // Special handling for keep_alive_interval to allow disabling it
    if let Ok(val) = std::env::var("FORGE_HTTP_KEEP_ALIVE_INTERVAL") {
        if val.to_lowercase() == "none" || val.to_lowercase() == "disabled" {
            config.keep_alive_interval = None;
        } else if let Some(parsed) = parse_env::<u64>("FORGE_HTTP_KEEP_ALIVE_INTERVAL") {
            config.keep_alive_interval = Some(parsed);
        }
    }

    if let Some(parsed) = parse_env::<u64>("FORGE_HTTP_KEEP_ALIVE_TIMEOUT") {
        config.keep_alive_timeout = parsed;
    }
    if let Some(parsed) = parse_env::<bool>("FORGE_HTTP_KEEP_ALIVE_WHILE_IDLE") {
        config.keep_alive_while_idle = parsed;
    }
    if let Some(parsed) = parse_env::<bool>("FORGE_HTTP_ACCEPT_INVALID_CERTS") {
        config.accept_invalid_certs = parsed;
    }
    if let Some(val) = parse_env::<String>("FORGE_HTTP_ROOT_CERT_PATHS") {
        let paths: Vec<String> = val
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !paths.is_empty() {
            config.root_cert_paths = Some(paths);
        }
    }

    config
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::{env, fs};

    use forge_domain::{TlsBackend, TlsVersion};
    use pretty_assertions::assert_eq;
    use serial_test::serial;
    use tempfile::{TempDir, tempdir};

    use super::*;

    fn setup_envs(structure: Vec<(&str, &str)>) -> (TempDir, PathBuf) {
        let root = tempdir().unwrap();
        let root_path = root.path().to_path_buf();

        for (rel_path, content) in &structure {
            let dir = root_path.join(rel_path);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(".env"), content).unwrap();
        }

        let deepest_path = root_path.join(structure[0].0);
        // We MUST return root path, because dropping it will remove temp dir
        (root, deepest_path)
    }

    fn clean_retry_env_vars() {
        let retry_env_vars = [
            "FORGE_RETRY_INITIAL_BACKOFF_MS",
            "FORGE_RETRY_BACKOFF_FACTOR",
            "FORGE_RETRY_MAX_ATTEMPTS",
            "FORGE_RETRY_STATUS_CODES",
            "FORGE_SUPPRESS_RETRY_ERRORS",
        ];

        for var in &retry_env_vars {
            unsafe {
                env::remove_var(var);
            }
        }
    }

    fn clean_http_env_vars() {
        let http_env_vars = [
            "FORGE_HTTP_CONNECT_TIMEOUT",
            "FORGE_HTTP_READ_TIMEOUT",
            "FORGE_HTTP_POOL_IDLE_TIMEOUT",
            "FORGE_HTTP_POOL_MAX_IDLE_PER_HOST",
            "FORGE_HTTP_MAX_REDIRECTS",
            "FORGE_HTTP_USE_HICKORY",
            "FORGE_HTTP_TLS_BACKEND",
            "FORGE_HTTP_MIN_TLS_VERSION",
            "FORGE_HTTP_MAX_TLS_VERSION",
            "FORGE_HTTP_ADAPTIVE_WINDOW",
            "FORGE_HTTP_KEEP_ALIVE_INTERVAL",
            "FORGE_HTTP_KEEP_ALIVE_TIMEOUT",
            "FORGE_HTTP_KEEP_ALIVE_WHILE_IDLE",
            "FORGE_HTTP_ACCEPT_INVALID_CERTS",
            "FORGE_HTTP_ROOT_CERT_PATHS",
        ];

        for var in &http_env_vars {
            unsafe {
                env::remove_var(var);
            }
        }
    }

    #[test]
    #[serial]
    fn test_dot_env_loading() {
        // Test single env file
        let (_root, cwd) = setup_envs(vec![("", "TEST_KEY1=VALUE1")]);
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("TEST_KEY1").unwrap(), "VALUE1");

        // Test nested env files with override (closer files win)
        let (_root, cwd) = setup_envs(vec![("a/b", "TEST_KEY2=SUB"), ("a", "TEST_KEY2=ROOT")]);
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("TEST_KEY2").unwrap(), "SUB");

        // Test multiple keys from different levels
        let (_root, cwd) = setup_envs(vec![
            ("a/b", "SUB_KEY3=SUB_VAL"),
            ("a", "ROOT_KEY3=ROOT_VAL"),
        ]);
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("ROOT_KEY3").unwrap(), "ROOT_VAL");
        assert_eq!(env::var("SUB_KEY3").unwrap(), "SUB_VAL");

        // Test standard env precedence (std env wins over .env files)
        let (_root, cwd) = setup_envs(vec![("a/b", "TEST_KEY4=SUB_VAL")]);
        unsafe {
            env::set_var("TEST_KEY4", "STD_ENV_VAL");
        }
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("TEST_KEY4").unwrap(), "STD_ENV_VAL");
    }

    #[test]
    #[serial]
    fn test_retry_config_parsing() {
        clean_retry_env_vars();

        // Test defaults
        let actual = resolve_retry_config();
        let expected = RetryConfig::default();
        assert_eq!(actual.max_retry_attempts, expected.max_retry_attempts);
        assert_eq!(actual.initial_backoff_ms, expected.initial_backoff_ms);
        assert_eq!(actual.backoff_factor, expected.backoff_factor);
        assert_eq!(actual.retry_status_codes, expected.retry_status_codes);
        assert_eq!(actual.suppress_retry_errors, expected.suppress_retry_errors);

        // Test environment variable overrides
        unsafe {
            env::set_var("FORGE_RETRY_INITIAL_BACKOFF_MS", "500");
            env::set_var("FORGE_RETRY_BACKOFF_FACTOR", "3");
            env::set_var("FORGE_RETRY_MAX_ATTEMPTS", "5");
            env::set_var("FORGE_RETRY_STATUS_CODES", "429,500,502");
            env::set_var("FORGE_SUPPRESS_RETRY_ERRORS", "true");
        }

        let actual = resolve_retry_config();
        assert_eq!(actual.initial_backoff_ms, 500);
        assert_eq!(actual.backoff_factor, 3);
        assert_eq!(actual.max_retry_attempts, 5);
        assert_eq!(actual.retry_status_codes, vec![429, 500, 502]);
        assert!(actual.suppress_retry_errors);

        clean_retry_env_vars();
    }

    #[test]
    #[serial]
    fn test_retry_config_invalid_values() {
        clean_retry_env_vars();

        // Set invalid values - should fallback to defaults
        unsafe {
            env::set_var("FORGE_RETRY_INITIAL_BACKOFF_MS", "invalid");
            env::set_var("FORGE_RETRY_MAX_ATTEMPTS", "abc");
            env::set_var("FORGE_RETRY_STATUS_CODES", "invalid,codes");
        }

        let actual = resolve_retry_config();
        let expected = RetryConfig::default();
        assert_eq!(actual.initial_backoff_ms, expected.initial_backoff_ms);
        assert_eq!(actual.max_retry_attempts, expected.max_retry_attempts);
        assert_eq!(actual.retry_status_codes, expected.retry_status_codes);

        clean_retry_env_vars();
    }

    #[test]
    #[serial]
    fn test_http_config_parsing() {
        clean_http_env_vars();

        // Test defaults
        let actual = resolve_http_config();
        let expected = forge_domain::HttpConfig::default();
        assert_eq!(actual.connect_timeout, expected.connect_timeout);
        assert_eq!(actual.read_timeout, expected.read_timeout);
        assert_eq!(actual.tls_backend, expected.tls_backend);
        assert_eq!(actual.hickory, expected.hickory);
        assert_eq!(actual.accept_invalid_certs, expected.accept_invalid_certs);
        assert_eq!(actual.root_cert_paths, expected.root_cert_paths);

        // Test environment variable overrides
        unsafe {
            env::set_var("FORGE_HTTP_CONNECT_TIMEOUT", "30");
            env::set_var("FORGE_HTTP_USE_HICKORY", "true");
            env::set_var("FORGE_HTTP_TLS_BACKEND", "rustls");
            env::set_var("FORGE_HTTP_MIN_TLS_VERSION", "1.2");
            env::set_var("FORGE_HTTP_KEEP_ALIVE_INTERVAL", "30");
            env::set_var("FORGE_HTTP_ACCEPT_INVALID_CERTS", "true");
            env::set_var(
                "FORGE_HTTP_ROOT_CERT_PATHS",
                "/path/to/cert1.pem,/path/to/cert2.crt",
            );
        }

        let actual = resolve_http_config();
        assert_eq!(actual.connect_timeout, 30);
        assert!(actual.hickory);
        assert_eq!(actual.tls_backend, TlsBackend::Rustls);
        assert_eq!(actual.min_tls_version, Some(TlsVersion::V1_2));
        assert_eq!(actual.keep_alive_interval, Some(30));
        assert!(actual.accept_invalid_certs);
        assert_eq!(
            actual.root_cert_paths,
            Some(vec![
                "/path/to/cert1.pem".to_string(),
                "/path/to/cert2.crt".to_string()
            ])
        );

        clean_http_env_vars();
    }

    #[test]
    #[serial]
    fn test_http_config_keep_alive_special_cases() {
        clean_http_env_vars();

        // Test "none" and "disabled" values disable keep_alive_interval
        for disable_value in ["none", "disabled", "NONE", "DISABLED"] {
            unsafe {
                env::set_var("FORGE_HTTP_KEEP_ALIVE_INTERVAL", disable_value);
            }
            let actual = resolve_http_config();
            assert_eq!(actual.keep_alive_interval, None);
        }

        clean_http_env_vars();
    }

    #[test]
    #[serial]
    fn test_max_search_result_bytes() {
        unsafe {
            env::remove_var("FORGE_MAX_SEARCH_RESULT_BYTES");
        }

        let infra = ForgeEnvironmentInfra::new(false, PathBuf::from("/tmp"));

        // Test default value — driven by ForgeConfig defaults, not env vars
        let environment = infra.get_environment();
        // ForgeConfig::default() sets max_search_result_bytes to some value;
        // just assert it's non-zero
        assert!(environment.max_search_result_bytes > 0);

        unsafe {
            env::remove_var("FORGE_MAX_SEARCH_RESULT_BYTES");
        }
    }

    #[test]
    #[serial]
    fn test_auto_open_dump_env_var() {
        let cwd = tempdir().unwrap().path().to_path_buf();
        let infra = ForgeEnvironmentInfra::new(false, cwd);

        // Test default value when env var is not set
        {
            unsafe {
                env::remove_var("FORGE_DUMP_AUTO_OPEN");
            }
            let env = infra.get_environment();
            assert!(!env.auto_open_dump);
        }
    }

    #[test]
    #[serial]
    fn test_auto_dump_env_var() {
        let cwd = tempdir().unwrap().path().to_path_buf();
        let infra = ForgeEnvironmentInfra::new(false, cwd);

        // Test default value when env var is not set
        {
            unsafe {
                env::remove_var("FORGE_AUTO_DUMP");
            }
            let env = infra.get_environment();
            assert_eq!(env.auto_dump, None);
        }
    }

    #[test]
    #[serial]
    fn test_tool_timeout_env_var() {
        let cwd = tempdir().unwrap().path().to_path_buf();
        let infra = ForgeEnvironmentInfra::new(false, cwd);

        // Test default value when env var is not set
        {
            unsafe {
                env::remove_var("FORGE_TOOL_TIMEOUT");
            }
            let env = infra.get_environment();
            assert!(env.tool_timeout > 0);
        }
    }

    #[test]
    #[serial]
    fn test_max_conversations_env_var() {
        let cwd = tempfile::tempdir().unwrap();
        let infra = ForgeEnvironmentInfra::new(false, cwd.path().to_path_buf());

        // Test default value
        unsafe {
            std::env::remove_var("FORGE_MAX_CONVERSATIONS");
        }
        let env = infra.get_environment();
        assert!(env.max_conversations > 0);

        unsafe {
            std::env::remove_var("FORGE_MAX_CONVERSATIONS");
        }
    }

    #[test]
    #[serial]
    fn test_multiline_env_vars() {
        let content = r#"MULTI_LINE='line1
line2
line3'
SIMPLE=value"#;

        let (_root, cwd) = setup_envs(vec![("", content)]);
        ForgeEnvironmentInfra::dot_env(&cwd);

        // Verify multiline variable
        let multi = env::var("MULTI_LINE").expect("MULTI_LINE should be set");
        assert_eq!(multi, "line1\nline2\nline3");

        // Verify simple var
        assert_eq!(env::var("SIMPLE").unwrap(), "value");

        unsafe {
            env::remove_var("MULTI_LINE");
            env::remove_var("SIMPLE");
        }
    }

    #[test]
    #[serial]
    fn test_unified_parse_env_functionality() {
        // Test boolean parsing with custom logic
        unsafe {
            env::set_var("TEST_BOOL_TRUE", "yes");
            env::set_var("TEST_BOOL_FALSE", "no");
        }

        assert_eq!(parse_env::<bool>("TEST_BOOL_TRUE"), Some(true));
        assert_eq!(parse_env::<bool>("TEST_BOOL_FALSE"), Some(false));

        // Test numeric parsing
        unsafe {
            env::set_var("TEST_U64", "123");
            env::set_var("TEST_F64", "45.67");
        }

        assert_eq!(parse_env::<u64>("TEST_U64"), Some(123));
        assert_eq!(parse_env::<f64>("TEST_F64"), Some(45.67));

        // Test string parsing
        unsafe {
            env::set_var("TEST_STRING", "hello world");
        }

        assert_eq!(
            parse_env::<String>("TEST_STRING"),
            Some("hello world".to_string())
        );

        // Test missing env var
        assert_eq!(parse_env::<bool>("NONEXISTENT_VAR"), None);
        assert_eq!(parse_env::<u64>("NONEXISTENT_VAR"), None);

        // Clean up
        unsafe {
            env::remove_var("TEST_BOOL_TRUE");
            env::remove_var("TEST_BOOL_FALSE");
            env::remove_var("TEST_U64");
            env::remove_var("TEST_F64");
            env::remove_var("TEST_STRING");
        }
    }
}
