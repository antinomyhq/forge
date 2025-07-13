use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let secret = env::var("FORGE_PRIVATE_KEY")
        .expect("FORGE_PRIVATE_KEY not set in env");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("secret.rs");

    // Escape quotes and write a Rust constant
    fs::write(
        &dest_path,
        format!(
            "pub const FORGE_PRIVATE_KEY: &str = \"{}\";",
            secret.replace('\\', "\\\\").replace('"', "\\\"")
        ),
    ).unwrap();

    println!("cargo:rerun-if-env-changed=FORGE_PRIVATE_KEY");
}
