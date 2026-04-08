use gh_workflow::*;

/// Creates a step to setup the Protobuf compiler.
///
/// Installs protoc via apt-get on Linux runners. This replaces
/// `arduino/setup-protoc` which is deprecated (Node.js 20).
pub fn setup_protoc() -> Step<Run> {
    Step::new("Setup Protobuf Compiler").run("sudo apt-get install -y protobuf-compiler")
}
