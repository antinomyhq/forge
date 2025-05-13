use gh_workflow_tailcall::*;

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

    npm_workflow = npm_workflow.add_job("npm_release", create_npm_release_job());

    npm_workflow
}

/// Create an NPM release job
pub fn create_npm_release_job() -> Job {
    Job::new("npm_release")
        .runs_on("ubuntu-latest")
        .add_step(
            Step::uses("actions", "checkout", "v4")
                .add_with(("repository", "antinomyhq/npm-code-forge"))
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
