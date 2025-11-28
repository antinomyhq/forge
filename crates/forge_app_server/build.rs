use std::path::PathBuf;

fn main() {
    // Output directory for generated TypeScript types
    let out_dir = PathBuf::from("../../vscode-extension/src/generated");

    // Ensure the directory exists
    std::fs::create_dir_all(&out_dir).expect("Failed to create output directory");

    println!("cargo:rerun-if-changed=src/protocol");
    println!(
        "cargo:warning=TypeScript types will be generated to {:?}",
        out_dir
    );
}
