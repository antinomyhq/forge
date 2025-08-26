use gh_workflow_tailcall::*;
use indexmap::indexmap;

/// Create a draft release job for GitHub Actions
pub fn create_draft_release_job(build_job: &Job) -> Job {
    Job::new("draft_release")
        .name("Draft Release")
        .runs_on("ubuntu-latest")
        .add_needs(build_job.clone())
        .cond(Expression::new(
            "github.event_name == 'push' && github.ref == 'refs/heads/main'",
        ))
        // This job only runs on push to main, not needed for release events
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write),
        )
        .add_step(Step::uses("actions", "checkout", "v5"))
        .add_step(
            Step::uses("release-drafter", "release-drafter", "v6")
                .id("create_release")
                .env(("GITHUB_TOKEN", "${{ secrets.GITHUB_TOKEN }}"))
                .with(("config-name", "release-drafter.yml")),
        )
        .add_step(
            Step::run("echo \"crate_release_id=${{ steps.create_release.outputs.id }}\" >> $GITHUB_OUTPUT && echo \"crate_release_name=${{ steps.create_release.outputs.tag_name }}\" >> $GITHUB_OUTPUT")
                .id("set_output"),
        )
        .outputs(indexmap! {
            "crate_release_name".to_string() => "${{ steps.set_output.outputs.crate_release_name }}".to_string(),
            "crate_release_id".to_string() => "${{ steps.set_output.outputs.crate_release_id }}".to_string()
        })
}
