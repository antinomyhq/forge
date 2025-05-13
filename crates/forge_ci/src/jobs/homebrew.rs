use gh_workflow_tailcall::*;

/// Create a workflow for homebrew releases
pub fn create_homebrew_workflow() -> Workflow {
    let mut homebrew_workflow = Workflow::default()
        .name("Homebrew Release")
        .on(Event {
            release: Some(Release { types: vec![ReleaseType::Published] }),
            ..Event::default()
        })
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write),
        );

    homebrew_workflow =
        homebrew_workflow.add_job("homebrew_release", create_homebrew_release_job());

    homebrew_workflow
}

/// Create a homebrew release job
pub fn create_homebrew_release_job() -> Job {
    Job::new("homebrew_release")
        .runs_on("ubuntu-latest")
        .add_step(
            Step::uses("actions", "checkout", "v4")
                .add_with(("repository", "antinomyhq/homebrew-code-forge"))
                .add_with(("ref", "main"))
                .add_with(("token", "${{ secrets.HOMEBREW_ACCESS }}")),
        )
        // Make script executable and run it with token
        .add_step(
            Step::run("GITHUB_TOKEN=\"${{ secrets.HOMEBREW_ACCESS }}\" ./update-formula.sh ${{ github.event.release.tag_name }}"),
        )
}
