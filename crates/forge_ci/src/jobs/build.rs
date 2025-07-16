use gh_workflow_tailcall::*;

use crate::{jobs::release_build_job::release_build_job, release_matrix::ReleaseMatrix};




/// Create a build job for drafts
pub fn create_build_release_job_for_publishing() -> Job {
    release_build_job()
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

/// Create a build job for PRs
fn create_build_release_job(draft_release_job: &Job) -> Job {
    release_build_job()
        .add_needs(draft_release_job.clone())
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
    create_build_release_job( draft_release_job).cond(Expression::new(
        "github.event_name == 'pull_request' && contains(github.event.pull_request.labels.*.name, 'build-all-targets')",
    ))
}

/// Create a build job for main branch that adds binaries to release
pub fn create_build_release_main_job(draft_release_job: &Job) -> Job {
    create_build_release_job(draft_release_job)
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
                    "${{ needs.draft_release.outputs.crate_release_id }}",
                ))
                .add_with(("file", "${{ matrix.binary_name }}"))
                .add_with(("overwrite", "true")),
        )
}

#[cfg(test)]
mod test {
    use crate::jobs::apt_get_install;

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
