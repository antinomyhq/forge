use gh_workflow::*;

/// Creates a step to setup the Protobuf compiler.
///
/// Installs protoc on Linux (apt-get), macOS (brew), and Windows (choco).
/// Uses `shell: bash` to ensure consistent behavior across all runners.
/// This replaces `arduino/setup-protoc` which is deprecated (Node.js 20).
pub fn setup_protoc() -> Step<Run> {
    let mut step = Step::new("Setup Protobuf Compiler")
        .run("if command -v apt-get >/dev/null 2>&1; then sudo apt-get install -y protobuf-compiler; elif command -v brew >/dev/null 2>&1; then brew install protobuf; elif command -v choco >/dev/null 2>&1; then choco install protoc -y; fi");
    step.value.shell = Some("bash".to_string());
    step
}
