use gh_workflow::*;

/// Creates a step to setup the Protobuf compiler.
///
/// Installs protoc on Linux (apt-get) and macOS (brew). This replaces
/// `arduino/setup-protoc` which is deprecated (Node.js 20).
pub fn setup_protoc() -> Step<Run> {
    Step::new("Setup Protobuf Compiler").run(
        "if command -v apt-get >/dev/null 2>&1; then sudo apt-get install -y protobuf-compiler; elif command -v brew >/dev/null 2>&1; then brew install protobuf; fi",
    )
}
