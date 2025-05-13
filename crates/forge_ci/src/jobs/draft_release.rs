use gh_workflow_tailcall::*;
use indexmap::indexmap;

/// Create a draft release job for GitHub Actions
pub fn create_draft_release_job() -> Job {
    Job::new("draft_release")
        .runs_on("ubuntu-latest")
        .cond(Expression::new(
            "github.event_name == 'push' && github.ref == 'refs/heads/main'",
        ))
        // This job only runs on push to main, not needed for release events
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write),
        )
        .add_step(Step::uses("actions", "checkout", "v4"))
        .add_step(
            Step::uses("release-drafter", "release-drafter", "v6")
                .id("create_release")
                .env(("GITHUB_TOKEN", "${{ secrets.GITHUB_TOKEN }}"))
                .with(("config-name", "release-drafter.yml")),
        )
        .add_step(
            Step::run("echo \"create_release_id=${{ steps.create_release.outputs.id }}\" >> $GITHUB_OUTPUT && echo \"create_release_name=${GITHUB_REF#refs/tags/}\" >> $GITHUB_OUTPUT")
                .id("set_output"),
        )
        .outputs(indexmap! {
            "create_release_name".to_string() => "${{ steps.set_output.outputs.create_release_name }}".to_string(),
            "create_release_id".to_string() => "${{ steps.set_output.outputs.create_release_id }}".to_string()
        })
}
