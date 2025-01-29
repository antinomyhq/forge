use gh_workflow_tailcall::*;
use serde_json::json;

#[test]
fn generate() {
    let mut workflow = StandardWorkflow::default()
        .auto_fix(true)
        .add_setup(Step::run("sudo apt-get install -y libsqlite3-dev"))
        .to_ci_workflow()
        .add_env(("FORGE_KEY", "${{secrets.FORGE_KEY}}"));

    // Set up the build matrix for all platforms
    let matrix = json!({
        "include": [
            {
                "os": "ubuntu-latest",
                "target": "x86_64-unknown-linux-gnu",
                "binary_name": "forge-x86_64-unknown-linux-gnu",
                "binary_path": "target/x86_64-unknown-linux-gnu/release/forge"
            },
            {
                "os": "macos-latest",
                "target": "x86_64-apple-darwin",
                "binary_name": "forge-x86_64-apple-darwin",
                "binary_path": "target/x86_64-apple-darwin/release/forge"
            },
            {
                "os": "macos-latest",
                "target": "aarch64-apple-darwin",
                "binary_name": "forge-aarch64-apple-darwin",
                "binary_path": "target/aarch64-apple-darwin/release/forge"
            },
            {
                "os": "windows-latest",
                "target": "x86_64-pc-windows-msvc",
                "binary_name": "forge-x86_64-pc-windows-msvc.exe",
                "binary_path": "target/x86_64-pc-windows-msvc/release/forge.exe"
            }
        ]
    });

    let build_job = workflow.jobs.clone().unwrap().get("build").unwrap().clone();
    let main_cond =
        Expression::new("github.event_name == 'push' && github.ref == 'refs/heads/main'");

    // Add release build job
    workflow = workflow.add_job(
        "build-release",
        Job::new("build-release")
            .add_needs(build_job.clone())
            .cond(main_cond)
            .strategy(Strategy { fail_fast: None, max_parallel: None, matrix: Some(matrix) })
            .runs_on("${{ matrix.os }}")
            .add_step(Step::uses("actions", "checkout", "v4"))
            // Install Rust with cross-compilation target
            .add_step(
                Step::uses("dtolnay", "rust-toolchain", "stable")
                    .with(("targets", "${{ matrix.target }}")),
            )
            // Build release binary
            .add_step(
                Step::uses("ClementTsang", "cargo-action", "v0.0.3")
                    .add_with(("command", "build --release"))
                    .add_with(("args", "--target ${{ matrix.target }}")),
            )
            // Upload artifact for release
            .add_step(
                Step::uses("actions", "upload-artifact", "v3")
                    .add_with(("name", "${{ matrix.binary_name }}"))
                    .add_with(("path", "${{ inputs.path }}/${{ matrix.binary_name }}"))
                    .add_with(("if-no-files-found", "error")),
            ),
    );
    // Add release creation job
    let build_release_job = workflow
        .jobs
        .clone()
        .unwrap()
        .get("build-release")
        .unwrap()
        .clone();
    workflow = workflow.add_job(
        "create-release",
        Job::new("create-release")
            .add_needs(build_release_job)
            .runs_on("ubuntu-latest")
            .add_step(Step::uses("actions", "checkout", "v4"))
            // Download all artifacts
            .add_step(
                Step::uses("actions", "download-artifact", "v3")
                    .add_with(("name", "${{ matrix.binary_name }}"))
                    .add_with(("path", "${{ inputs.path }}")),
            )
            // Create GitHub release
            .add_step(
                Step::uses("softprops", "action-gh-release", "v1")
                    .add_with(("generate_release_notes", "true"))
                    .add_with(("files", "${{ inputs.path }}/artifacts/**/*"))
                    .add_with(("prerelease", "true"))
                    .add_with(("token", "${{ secrets.GITHUB_TOKEN }}")),
            ),
    );

    workflow.generate().unwrap();
}
