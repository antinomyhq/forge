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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::{env, fs};

    use forge_config::ForgeConfig;
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

    #[test]
    #[serial]
    fn test_dot_env_loading() {
        // Single env file
        let (_root, cwd) = setup_envs(vec![("", "TEST_KEY1=VALUE1")]);
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("TEST_KEY1").unwrap(), "VALUE1");

        // Nested env files with override (closer files win)
        let (_root, cwd) = setup_envs(vec![("a/b", "TEST_KEY2=SUB"), ("a", "TEST_KEY2=ROOT")]);
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("TEST_KEY2").unwrap(), "SUB");

        // Multiple keys from different levels
        let (_root, cwd) = setup_envs(vec![
            ("a/b", "SUB_KEY3=SUB_VAL"),
            ("a", "ROOT_KEY3=ROOT_VAL"),
        ]);
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("ROOT_KEY3").unwrap(), "ROOT_VAL");
        assert_eq!(env::var("SUB_KEY3").unwrap(), "SUB_VAL");

        // Standard env precedence (std env wins over .env files)
        let (_root, cwd) = setup_envs(vec![("a/b", "TEST_KEY4=SUB_VAL")]);
        unsafe {
            env::set_var("TEST_KEY4", "STD_ENV_VAL");
        }
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("TEST_KEY4").unwrap(), "STD_ENV_VAL");

        // Multiline values
        let content = r#"MULTI_LINE='line1
line2
line3'
SIMPLE=value"#;
        let (_root, cwd) = setup_envs(vec![("", content)]);
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(
            env::var("MULTI_LINE").expect("MULTI_LINE should be set"),
            "line1\nline2\nline3"
        );
        assert_eq!(env::var("SIMPLE").unwrap(), "value");

        unsafe {
            env::remove_var("MULTI_LINE");
            env::remove_var("SIMPLE");
        }
    }

    #[test]
    fn test_to_environment_default_config() {
        let fixture = ForgeConfig::default();
        let actual = to_environment(fixture, false, PathBuf::from("/test/cwd"));

        // Config-derived fields should all be zero/default since ForgeConfig
        // derives Default (all-zeros) without the defaults file.
        assert_eq!(actual.cwd, PathBuf::from("/test/cwd"));
        assert!(!actual.is_restricted);
        assert_eq!(actual.retry_config, RetryConfig::default());
        assert_eq!(actual.http, HttpConfig::default());
        assert!(!actual.auto_open_dump);
        assert_eq!(actual.auto_dump, None);
        assert_eq!(actual.debug_requests, None);
        assert_eq!(actual.custom_history_path, None);
        assert_eq!(actual.session, None);
        assert_eq!(actual.commit, None);
        assert_eq!(actual.suggest, None);
    }

    #[test]
    fn test_to_environment_restricted_mode() {
        let fixture = ForgeConfig::default();
        let actual = to_environment(fixture, true, PathBuf::from("/tmp"));

        assert!(actual.is_restricted);
    }

    #[test]
    fn test_to_environment_maps_all_config_fields() {
        let fixture = ForgeConfig {
            max_search_lines: 500,
            max_search_result_bytes: 2048,
            max_fetch_chars: 10_000,
            max_stdout_prefix_lines: 50,
            max_stdout_suffix_lines: 75,
            max_stdout_line_chars: 300,
            max_line_chars: 1500,
            max_read_lines: 3000,
            max_file_read_batch_size: 25,
            max_file_size_bytes: 5_000_000,
            max_image_size_bytes: 100_000,
            tool_timeout_secs: 120,
            auto_open_dump: true,
            max_conversations: 42,
            max_sem_search_results: 77,
            sem_search_top_k: 5,
            max_extensions: 10,
            max_parallel_file_reads: 32,
            model_cache_ttl_secs: 3600,
            ..ForgeConfig::default()
        };

        let actual = to_environment(fixture, false, PathBuf::from("/work"));

        assert_eq!(actual.max_search_lines, 500);
        assert_eq!(actual.max_search_result_bytes, 2048);
        assert_eq!(actual.fetch_truncation_limit, 10_000);
        assert_eq!(actual.stdout_max_prefix_length, 50);
        assert_eq!(actual.stdout_max_suffix_length, 75);
        assert_eq!(actual.stdout_max_line_length, 300);
        assert_eq!(actual.max_line_length, 1500);
        assert_eq!(actual.max_read_size, 3000);
        assert_eq!(actual.max_file_read_batch_size, 25);
        assert_eq!(actual.max_file_size, 5_000_000);
        assert_eq!(actual.max_image_size, 100_000);
        assert_eq!(actual.tool_timeout, 120);
        assert!(actual.auto_open_dump);
        assert_eq!(actual.max_conversations, 42);
        assert_eq!(actual.sem_search_limit, 77);
        assert_eq!(actual.sem_search_top_k, 5);
        assert_eq!(actual.max_extensions, 10);
        assert_eq!(actual.parallel_file_reads, 32);
        assert_eq!(actual.model_cache_ttl, 3600);
    }

    #[test]
    fn test_forge_config_round_trip() {
        let fixture = ForgeConfig {
            max_search_lines: 500,
            max_search_result_bytes: 2048,
            max_fetch_chars: 10_000,
            max_stdout_prefix_lines: 50,
            max_stdout_suffix_lines: 75,
            max_stdout_line_chars: 300,
            max_line_chars: 1500,
            max_read_lines: 3000,
            max_file_read_batch_size: 25,
            max_file_size_bytes: 5_000_000,
            max_image_size_bytes: 100_000,
            tool_timeout_secs: 120,
            auto_open_dump: true,
            max_conversations: 42,
            max_sem_search_results: 77,
            sem_search_top_k: 5,
            max_extensions: 10,
            max_parallel_file_reads: 32,
            model_cache_ttl_secs: 3600,
            ..ForgeConfig::default()
        };

        let env = to_environment(fixture.clone(), false, PathBuf::from("/work"));
        let actual = to_forge_config(&env);

        // Round-tripped fields should match the original
        assert_eq!(actual.max_search_lines, fixture.max_search_lines);
        assert_eq!(
            actual.max_search_result_bytes,
            fixture.max_search_result_bytes
        );
        assert_eq!(actual.max_fetch_chars, fixture.max_fetch_chars);
        assert_eq!(
            actual.max_stdout_prefix_lines,
            fixture.max_stdout_prefix_lines
        );
        assert_eq!(
            actual.max_stdout_suffix_lines,
            fixture.max_stdout_suffix_lines
        );
        assert_eq!(actual.max_stdout_line_chars, fixture.max_stdout_line_chars);
        assert_eq!(actual.max_line_chars, fixture.max_line_chars);
        assert_eq!(actual.max_read_lines, fixture.max_read_lines);
        assert_eq!(
            actual.max_file_read_batch_size,
            fixture.max_file_read_batch_size
        );
        assert_eq!(actual.max_file_size_bytes, fixture.max_file_size_bytes);
        assert_eq!(actual.max_image_size_bytes, fixture.max_image_size_bytes);
        assert_eq!(actual.tool_timeout_secs, fixture.tool_timeout_secs);
        assert_eq!(actual.auto_open_dump, fixture.auto_open_dump);
        assert_eq!(actual.max_conversations, fixture.max_conversations);
        assert_eq!(
            actual.max_sem_search_results,
            fixture.max_sem_search_results
        );
        assert_eq!(actual.sem_search_top_k, fixture.sem_search_top_k);
        assert_eq!(actual.max_extensions, fixture.max_extensions);
        assert_eq!(
            actual.max_parallel_file_reads,
            fixture.max_parallel_file_reads
        );
        assert_eq!(actual.model_cache_ttl_secs, fixture.model_cache_ttl_secs);
    }

    #[test]
    fn test_forge_config_environment_identity() {
        // Identity property: for any ForgeConfig `fc`, the config-mapped fields
        // of the Environment produced by `to_environment(fc)` must survive a
        // full round-trip through `to_forge_config` and back unchanged.
        //
        //   fc  -->  env  -->  fc'  -->  env'
        //            ^                    ^
        //            |--- config fields --|  must be equal
        let fixture = ForgeConfig {
            max_search_lines: 999,
            max_search_result_bytes: 4096,
            max_fetch_chars: 20_000,
            max_stdout_prefix_lines: 111,
            max_stdout_suffix_lines: 222,
            max_stdout_line_chars: 333,
            max_line_chars: 444,
            max_read_lines: 555,
            max_file_read_batch_size: 66,
            max_file_size_bytes: 7_777_777,
            max_image_size_bytes: 88_888,
            tool_timeout_secs: 999,
            auto_open_dump: true,
            debug_requests: Some(PathBuf::from("/tmp/debug")),
            custom_history_path: Some(PathBuf::from("/custom/history")),
            max_conversations: 50,
            max_sem_search_results: 200,
            sem_search_top_k: 15,
            services_url: "https://custom.example.com".to_string(),
            max_extensions: 25,
            auto_dump: Some(forge_config::AutoDumpFormat::Html),
            max_parallel_file_reads: 128,
            model_cache_ttl_secs: 7200,
            retry: Some(forge_config::RetryConfig {
                initial_backoff_ms: 100,
                min_delay_ms: 50,
                backoff_factor: 3,
                max_attempts: 5,
                status_codes: vec![429, 503],
                max_delay_secs: Some(60),
                suppress_errors: true,
            }),
            http: Some(forge_config::HttpConfig {
                connect_timeout_secs: 10,
                read_timeout_secs: 30,
                pool_idle_timeout_secs: 90,
                pool_max_idle_per_host: 20,
                max_redirects: 5,
                hickory: true,
                tls_backend: forge_config::TlsBackend::Rustls,
                min_tls_version: Some(forge_config::TlsVersion::V1_2),
                max_tls_version: Some(forge_config::TlsVersion::V1_3),
                adaptive_window: true,
                keep_alive_interval_secs: Some(15),
                keep_alive_timeout_secs: 20,
                keep_alive_while_idle: true,
                accept_invalid_certs: true,
                root_cert_paths: Some(vec!["/etc/ssl/custom.pem".to_string()]),
            }),
            session: Some(ModelConfig {
                provider_id: Some("anthropic".to_string()),
                model_id: Some("claude-3".to_string()),
            }),
            commit: Some(ModelConfig {
                provider_id: Some("openai".to_string()),
                model_id: Some("gpt-4".to_string()),
            }),
            suggest: Some(ModelConfig {
                provider_id: Some("google".to_string()),
                model_id: Some("gemini".to_string()),
            }),
            ..ForgeConfig::default()
        };

        let cwd = PathBuf::from("/identity/test");
        let restricted = true;

        // fc -> env -> fc' -> env'
        let env = to_environment(fixture.clone(), restricted, cwd.clone());
        let fc_prime = to_forge_config(&env);
        let env_prime = to_environment(fc_prime, restricted, cwd);

        // Infrastructure-derived fields (os, pid, home, shell, base_path) are
        // re-derived from the runtime, so they are equal by construction.
        // Config-mapped fields must satisfy the identity: env == env'
        assert_eq!(env, env_prime);
    }
}
