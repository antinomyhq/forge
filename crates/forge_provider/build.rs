use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let secret = env::var("FORGE_PRIVATE_KEY");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("secret.rs");
    let secret = match secret {
        Ok(secret) => format!(
            "pub const FORGE_PRIVATE_KEY: Option<&str> = Some(\"{}\");",
            secret.replace('\\', "\\\\").replace('"', "\\\"")
        ),
        Err(_) => "pub const FORGE_PRIVATE_KEY: Option<&str> = None;".to_string(),
    };

    // Escape quotes and write a Rust constant
    fs::write(&dest_path, secret).unwrap();

    println!("cargo:rerun-if-env-changed=FORGE_PRIVATE_KEY");
}
