use gh_workflow_tailcall::*;

/// Create a job to update the release draft
pub fn draft_release_update_job() -> Job {
    Job::new("update_release_draft")
        .runs_on("ubuntu-latest")
        .add_step(
            Step::uses("release-drafter", "release-drafter", "v6")
                .env(("GITHUB_TOKEN", "${{ secrets.GITHUB_TOKEN }}"))
                .add_with(("config-name", "release-drafter.yml")),
        )
}
