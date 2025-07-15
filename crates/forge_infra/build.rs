use std::path::Path;
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MTLS_CERT");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("secret_cert.rs");

    let rhs = gen_cert();
    let generated = format!(
        r#"
lazy_static::lazy_static! {{
    pub static ref CERT: Option<String> = {rhs};
}}
"#
    );

    std::fs::write(dest_path, generated).unwrap();
}

fn gen_cert() -> String {
    if let Ok(cert_content) = std::env::var("MTLS_CERT") {
        format!(
            "Some(obfstr::obfstr!({:?}).to_string())",
            cert_content.trim()
        )
    } else {
        "None".to_string()
    }
}
