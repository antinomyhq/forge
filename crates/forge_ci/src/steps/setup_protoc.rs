use gh_workflow::*;

/// Creates a step to setup the Protobuf compiler.
///
/// This step is reusable across all CI workflows that need protobuf
/// compilation.
pub fn setup_protoc() -> Step<Use> {
    Step::new("Setup Protobuf Compiler")
        .uses("arduino", "setup-protoc", "v3")
        .with(("repo-token", "${{ secrets.GITHUB_TOKEN }}"))
}
