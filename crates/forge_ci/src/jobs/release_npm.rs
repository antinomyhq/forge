use gh_workflow_tailcall::*;
use serde_json::Value;

use crate::jobs::apt_get_install;
use crate::matrix;

/// Create a base build job that can be customized
fn create_build_release_job(matrix: Value) -> Job {
    Job::new("build-release")
        .strategy(Strategy { fail_fast: None, max_parallel: None, matrix: Some(matrix) })
        .runs_on("${{ matrix.os }}")
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write),
        )
        .add_step(Step::uses("actions", "checkout", "v4"))
        // Install Rust with cross-compilation target
        .add_step(
            Step::uses("taiki-e", "setup-cross-toolchain-action", "v1")
                .with(("target", "${{ matrix.target }}")),
        )
        // Explicitly add the target to ensure it's available
        .add_step(Step::run("rustup target add ${{ matrix.target }}").name("Add Rust target"))
        // Build add link flags
        .add_step(
            Step::run(r#"echo "RUSTFLAGS=-C target-feature=+crt-static" >> $GITHUB_ENV"#)
                .if_condition(Expression::new(
                    "!contains(matrix.target, '-unknown-linux-gnu')",
                )),
        )
        .add_step(
            Step::run(apt_get_install(&[
                "gcc-aarch64-linux-gnu",
                "musl-tools",
                "musl-dev",
                "pkg-config",
                "libssl-dev",
            ]))
            .if_condition(Expression::new(
                "contains(matrix.target, '-unknown-linux-musl')",
            )),
        )
        // Build release binary
        .add_step(
            Step::uses("ClementTsang", "cargo-action", "v0.0.6")
                .add_with(("command", "build --release"))
                .add_with(("args", "--target ${{ matrix.target }}"))
                .add_with(("use-cross", "${{ matrix.cross }}"))
                .add_with(("cross-version", "0.2.4"))
                .add_env(("RUSTFLAGS", "${{ env.RUSTFLAGS }}"))
                .add_env(("POSTHOG_API_SECRET", "${{secrets.POSTHOG_API_SECRET}}"))
                .add_env(("APP_VERSION", "${{ github.event.release.tag_name }}")),
        )
        // Rename binary to target name
        .add_step(Step::run(
            "cp ${{ matrix.binary_path }} ${{ matrix.binary_name }}",
        ))
        // Upload directly to release
        .add_step(
            Step::uses("xresloader", "upload-to-github-release", "v1")
                .add_with(("release_id", "${{ github.event.release.id }}"))
                .add_with(("file", "${{ matrix.binary_name }}"))
                .add_with(("overwrite", "true")),
        )
}

/// Create a workflow for NPM releases
pub fn create_npm_workflow() -> Workflow {
    let mut npm_workflow = Workflow::default()
        .name("NPM Release")
        .on(Event {
            release: Some(Release { types: vec![ReleaseType::Published] }),
            ..Event::default()
        })
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write),
        );

    let build_job = create_build_release_job(matrix::create_matrix());
    let npm_release_job = create_npm_release_job().add_needs(build_job.clone());

    npm_workflow = npm_workflow
        .add_job("build-release", build_job)
        .add_job("npm_release", npm_release_job);

    npm_workflow
}

/// Create an NPM release job using matrix strategy for multiple repositories
pub fn create_npm_release_job() -> Job {
    let matrix = create_npm_matrix();

    Job::new("npm_release")
        .strategy(Strategy { fail_fast: None, max_parallel: None, matrix: Some(matrix) })
        .runs_on("ubuntu-latest")
        .add_step(
            Step::uses("actions", "checkout", "v4")
                .add_with(("repository", "${{ matrix.repository }}"))
                .add_with(("ref", "main"))
                .add_with(("token", "${{ secrets.NPM_ACCESS }}")),
        )
        // Make script executable and run it with token
        .add_step(
            Step::run("./update-package.sh ${{ github.event.release.tag_name }}")
                .add_env(("AUTO_PUSH", "true"))
                .add_env(("CI", "true"))
                .add_env(("NPM_TOKEN", "${{ secrets.NPM_TOKEN }}")),
        )
}

/// Creates a matrix Value for NPM repositories
fn create_npm_matrix() -> Value {
    serde_json::json!({
        "repository": [
            "antinomyhq/npm-code-forge",
            "antinomyhq/npm-forgecode"
        ]
    })
}
