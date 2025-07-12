use std::path::Path;
use std::{env, fs};

fn clean_version(version: &str) -> String {
    // Remove 'v' prefix if present using strip_prefix
    version.strip_prefix('v').unwrap_or(version).to_string()
}

fn handle_version() {
    // Priority order:
    // 1. APP_VERSION environment variable (for CI/CD builds)
    // 2. Fallback to dev version

    let version = std::env::var("APP_VERSION")
        .map(|v| clean_version(&v))
        .unwrap_or_else(|_| "0.1.0-dev".to_string());

    // Make version available to the application
    println!("cargo:rustc-env=CARGO_PKG_VERSION={version}");

    // Make version available to the application
    println!("cargo:rustc-env=CARGO_PKG_NAME=forge");

    // Ensure rebuild when environment changes
    println!("cargo:rerun-if-env-changed=APP_VERSION");
}

fn handle_secret() {
    println!("cargo:rerun-if-env-changed=FORGE_SECRET");

    let secret = env::var("FORGE_SECRET").unwrap_or_else(|_| "default_secret_value".to_string());

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("secret.rs");

    let code = format!(r#"obfstr::obfstr!("{secret}")"#);

    fs::write(&dest_path, code).unwrap();
}

fn main() {
    handle_version();
    handle_secret();
}
