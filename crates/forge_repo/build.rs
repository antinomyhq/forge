fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Temporarily disabled to avoid protoc dependency during merge
    // tonic_prost_build::compile_protos("proto/forge.proto")?;
    Ok(())
}
