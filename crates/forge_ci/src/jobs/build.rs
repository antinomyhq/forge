use gh_workflow_tailcall::*;
use serde_json::Value;

use crate::matrix;

/// Helper function to generate an apt-get install command for multiple packages
fn apt_get_install(packages: &[&str]) -> String {
    format!(
        "sudo apt-get update && \\\nsudo apt-get install -y \\\n{}",
        packages
            .iter()
            .map(|pkg| format!("  {pkg}"))
            .collect::<Vec<_>>()
            .join(" \\\n")
    )
}

/// Create a base build job that can be customized
fn create_build_release_job(matrix: Value, draft_release_job: &Job) -> Job {
    Job::new("build-release")
        .add_needs(draft_release_job.clone())
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
                .add_env((
                    "APP_VERSION",
                    "${{ needs.draft_release.outputs.crate_release_name }}",
                )),
        )
}

/// Create a build job for PRs with the 'build-all-targets' label
pub fn create_build_release_pr_job(draft_release_job: &Job) -> Job {
    let matrix = matrix::create_matrix();
    create_build_release_job(matrix.clone(), draft_release_job).cond(Expression::new(
        "github.event_name == 'pull_request' && contains(github.event.pull_request.labels.*.name, 'build-all-targets')",
    ))
}

/// Create a build job for main branch that adds binaries to release
pub fn create_build_release_main_job(draft_release_job: &Job) -> Job {
    let matrix = matrix::create_matrix();
    create_build_release_job(matrix.clone(), draft_release_job)
        .cond(Expression::new(
            "(github.event_name == 'push' && github.ref == 'refs/heads/main')",
        ))
        // Rename binary to target name
        .add_step(Step::run(
            "cp ${{ matrix.binary_path }} ${{ matrix.binary_name }}",
        ))
        // Upload directly to release
        .add_step(
            Step::uses("xresloader", "upload-to-github-release", "v1")
                .add_with((
                    "release_id",
                    "${{ needs.draft_release.outputs.crate_release_name }}",
                ))
                .add_with(("file", "${{ matrix.binary_name }}"))
                .add_with(("overwrite", "true")),
        )
}

#[cfg(test)]
mod test {
    use crate::jobs::build::apt_get_install;

    #[test]
    fn test_apt_get_install() {
        let packages = &["pkg1", "pkg2", "pkg3"];
        let command = apt_get_install(packages);
        assert_eq!(
            command,
            "sudo apt-get update && \\\nsudo apt-get install -y \\\n  pkg1 \\\n  pkg2 \\\n  pkg3"
        );
    }
}
