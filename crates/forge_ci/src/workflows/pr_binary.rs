use gh_workflow_tailcall::generate::Generate;
use gh_workflow_tailcall::*;

/// Generate a workflow that builds binaries for PRs that can be downloaded and
/// tested locally
pub fn generate_pr_binary_workflow() {
    let workflow = Workflow::default()
        .name("PR Binary Build")
        .on(Event {
            pull_request: Some(PullRequest::default()),
            ..Event::default()
        })
        .concurrency(Concurrency {
            group: "${{ github.workflow }}-${{ github.ref }}".to_string(),
            cancel_in_progress: Some(true),
            limit: None,
        });

    // Create a PR binary build job that uploads artifacts instead of releases
    let pr_binary_job = Job::new("build-pr-binaries")
        .strategy(Strategy {
            fail_fast: Some(false),
            max_parallel: None,
            matrix: Some(serde_json::json!({
                "include": [
                    {
                "os": "ubuntu-latest",
                "target": "x86_64-unknown-linux-musl",
                "binary_name": "forge-x86_64-unknown-linux-musl",
                "binary_path": "target/x86_64-unknown-linux-musl/debug/forge",
                "cross": "false",
            }
                    // {
                    //     "os": "ubuntu-latest",
                    //     "target": "x86_64-unknown-linux-gnu",
                    //     "binary_path": "target/x86_64-unknown-linux-gnu/debug/forge",
                    //     "binary_name": "forge-linux-x86_64",
                    //     "cross": false
                    // },
                    // {
                    //     "os": "ubuntu-latest",
                    //     "target": "aarch64-unknown-linux-gnu",
                    //     "binary_path": "target/aarch64-unknown-linux-gnu/debug/forge",
                    //     "binary_name": "forge-linux-aarch64",
                    //     "cross": true
                    // },
                    // {
                    //     "os": "macos-latest",
                    //     "target": "x86_64-apple-darwin",
                    //     "binary_path": "target/x86_64-apple-darwin/debug/forge",
                    //     "binary_name": "forge-macos-x86_64",
                    //     "cross": false
                    // },
                    // {
                    //     "os": "macos-latest",
                    //     "target": "aarch64-apple-darwin",
                    //     "binary_path": "target/aarch64-apple-darwin/debug/forge",
                    //     "binary_name": "forge-macos-aarch64",
                    //     "cross": false
                    // },
                    // {
                    //     "os": "windows-latest",
                    //     "target": "x86_64-pc-windows-msvc",
                    //     "binary_path": "target/x86_64-pc-windows-msvc/debug/forge.exe",
                    //     "binary_name": "forge-windows-x86_64.exe",
                    //     "cross": false
                    // }
                ]
            })),
        })
        .runs_on("${{ matrix.os }}")
        .permissions(
            Permissions::default()
                .contents(Level::Read)
                .actions(Level::Read),
        )
        .add_step(Step::uses("actions", "checkout", "v4"))
        // Install Rust with cross-compilation target
        .add_step(
            Step::uses("taiki-e", "setup-cross-toolchain-action", "v1")
                .with(("target", "${{ matrix.target }}")),
        )
        // Explicitly add the target to ensure it's available
        .add_step(Step::run("rustup target add ${{ matrix.target }}").name("Add Rust target"))
        // Build add link flags for static linking with traditional static model (not static-PIE)
        .add_step(
            Step::run(r#"echo "RUSTFLAGS=-C target-feature=+crt-static -C relocation-model=static" >> $GITHUB_ENV"#)
                .if_condition(Expression::new(
                    "false",
                )),
        )
        // Install dependencies for cross-compilation on Linux
        .add_step(
            Step::run("sudo apt-get update && sudo apt-get install -y gcc-aarch64-linux-gnu")
                .if_condition(Expression::new(
                    "contains(matrix.target, 'aarch64-unknown-linux-gnu')",
                )),
        )
        // Build debug binary
        .add_step(
            Step::uses("ClementTsang", "cargo-action", "v0.0.6")
                .add_with(("command", "build"))
                .add_with(("args", "--target ${{ matrix.target }}"))
                .add_with(("use-cross", "${{ matrix.cross }}"))
                .add_with(("cross-version", "0.2.4"))
                .add_env(("RUSTFLAGS", "${{ env.RUSTFLAGS }}"))
                .add_env((
                    "APP_VERSION",
                    "pr-${{ github.event.pull_request.number }}-${{ github.sha }}",
                ))
                .name("Build binary"),
        )
        // Rename binary to target name
        .add_step(
            Step::run("cp '${{ matrix.binary_path }}' '${{ matrix.binary_name }}'")
                .name("Rename binary"),
        )
        // Upload binary as artifact for download
        .add_step(
            Step::uses("actions", "upload-artifact", "v4")
                .add_with(("name", "${{ matrix.binary_name }}"))
                .add_with(("path", "${{ matrix.binary_name }}"))
                .add_with(("retention-days", "7"))
                .name("Upload binary artifact"),
        );

    // Add the job to the workflow and generate it
    let workflow = workflow.add_job("build_pr_binaries", pr_binary_job);

    Generate::new(workflow)
        .name("pr-binary-build.yml")
        .generate()
        .unwrap();
}
