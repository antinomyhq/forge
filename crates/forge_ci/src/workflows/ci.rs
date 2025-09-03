use gh_workflow_tailcall::toolchain::Toolchain;
use gh_workflow_tailcall::*;

use crate::jobs::{self, ReleaseBuilderJob};

/// Generate the main CI workflow
pub fn generate_ci_workflow() {
    let workflow = StandardWorkflow::default()
        .auto_fix(true)
        .to_ci_workflow()
        .concurrency(Concurrency {
            group: "${{ github.workflow }}-${{ github.ref }}".to_string(),
            cancel_in_progress: None,
            limit: None,
        })
        .add_env(("OPENROUTER_API_KEY", "${{secrets.OPENROUTER_API_KEY}}"));

    // Replace the default build job to install nextest via taiki-e/install-action
    let custom_build = Job::new("Build and Test")
        .permissions(Permissions::default().contents(Level::Read))
        .add_step(Step::checkout())
        .add_step(Toolchain::default().add_stable())
        .add_step(
            Step::uses("taiki-e", "install-action", "v2")
                .name("Install nextest")
                .add_with(("tool", "cargo-nextest")),
        )
        .add_step(
            Step::uses("Swatinem", "rust-cache", "v2")
                .name("Cache Rust dependencies")
                .add_with(("cache-all-crates", "true")),
        )
        .add_step(
            Cargo::new("nextest")
                .args("run --all-features --workspace")
                .name("Cargo Nextest"),
        );

    let workflow = workflow.add_job("build", custom_build);

    // Replace the default lint job to pin nightly Clippy/Rustfmt to a known-good
    // version
    let custom_lint = Job::new("Lint")
        .permissions(Permissions::default().contents(Level::Read))
        .add_step(Step::checkout())
        .add_step(
            Step::uses("actions-rust-lang", "setup-rust-toolchain", "v1")
                .name("Setup Rust Toolchain")
                .add_with(("toolchain", "nightly-2025-09-02"))
                .add_with(("components", "clippy, rustfmt"))
                .add_with(("cache", "true"))
                .add_with((
                    "cache-directories",
                    "~/.cargo/registry\n~/.cargo/git\ntarget",
                )),
        )
        .add_step(Step::run("cargo +nightly fmt --all --check").name("Cargo Fmt"))
        .add_step(
            Step::run("cargo +nightly clippy --all-features --workspace -- -D warnings")
                .name("Cargo Clippy"),
        );

    let workflow = workflow.add_job("lint", custom_lint);

    // Get the jobs
    let build_job = workflow.jobs.clone().unwrap().get("build").unwrap().clone();
    let draft_release_job = jobs::create_draft_release_job(&build_job);

    // Add jobs to the workflow
    workflow
        .add_job("draft_release", draft_release_job.clone())
        .add_job(
            "build_release",
            ReleaseBuilderJob::new("${{ needs.draft_release.outputs.crate_release_name }}")
                .release_id("${{ needs.draft_release.outputs.crate_release_id }}")
                .into_job()
                .add_needs(draft_release_job.clone())
                .cond(Expression::new(
                    [
                        "github.event_name == 'push'",
                        "github.ref == 'refs/heads/main'",
                    ]
                    .join(" && "),
                )),
        )
        .add_job(
            "build_release_pr",
            ReleaseBuilderJob::new("${{ needs.draft_release.outputs.crate_release_name }}")
                .into_job()
                .add_needs(draft_release_job)
                .cond(Expression::new(
                    [
                        "github.event_name == 'pull_request'",
                        "contains(github.event.pull_request.labels.*.name, 'ci: build all targets')",
                    ]
                    .join(" && "),
                )),
        )
        .generate()
        .unwrap();
}
