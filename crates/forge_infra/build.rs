use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../cert.pem");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("secret_cert.rs");
    let file_path = Path::new("../../cert.pem");

    let rhs = gen_cert(&file_path);
    let generated = format!(
        r#"
lazy_static::lazy_static! {{
    pub static ref CERT: Option<String> = {};
}}
"#,
        rhs
    );

    std::fs::write(dest_path, generated).unwrap();
}

fn gen_cert(file_path: &Path) -> String {
    if file_path.exists() {
        let contents = std::fs::read_to_string(file_path).unwrap();
        format!("Some(obfstr::obfstr!({:?}).to_string())", contents.trim())
    } else {
        "None".to_string()
    }
}
