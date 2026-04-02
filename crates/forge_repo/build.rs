use std::error::Error;
use std::path::PathBuf;

/// Resolves the include directories needed to compile Forge's protobuf schema.
fn proto_includes() -> Result<Vec<PathBuf>, Box<dyn Error>> {
    // Always include the crate-local schema and the bundled well-known protobuf types.
    let includes = vec![PathBuf::from("proto"), protoc_bin_vendored::include_path()?];

    Ok(includes)
}

/// Ensures `prost-build` can invoke `protoc` even when it is not installed system-wide.
fn configure_protoc() -> Result<(), Box<dyn Error>> {
    // Respect an explicitly configured compiler and only install a fallback when absent.
    if std::env::var_os("PROTOC").is_none() {
        let protoc = protoc_bin_vendored::protoc_bin_path()?;
        unsafe {
            std::env::set_var("PROTOC", protoc);
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Make local Cargo builds self-sufficient without breaking existing CI or shell setup.
    configure_protoc()?;

    // Feed protoc both the project schema and protobuf's bundled well-known types.
    let includes = proto_includes()?;
    let protos = [PathBuf::from("proto/forge.proto")];
    tonic_prost_build::configure().compile_protos(&protos, &includes)?;

    Ok(())
}
