use gh_workflow_tailcall::*;

/// Create a Release Drafter workflow
pub fn create_release_drafter_workflow() -> Workflow {
    let mut release_drafter = Workflow::default()
        .name("Release Drafter")
        .on(Event {
            push: Some(Push { branches: vec!["main".to_string()], ..Push::default() }),
            pull_request_target: Some(PullRequestTarget {
                types: vec![
                    PullRequestType::Opened,
                    PullRequestType::Reopened,
                    PullRequestType::Synchronize,
                ],
                branches: vec!["main".to_string()],
            }),
            ..Event::default()
        })
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write),
        );

    release_drafter =
        release_drafter.add_job("update_release_draft", create_update_release_draft_job());

    release_drafter
}

/// Create a job to update the release draft
pub fn create_update_release_draft_job() -> Job {
    Job::new("update_release_draft")
        .runs_on("ubuntu-latest")
        .add_step(
            Step::uses("release-drafter", "release-drafter", "v6")
                .env(("GITHUB_TOKEN", "${{ secrets.GITHUB_TOKEN }}"))
                .add_with(("config-name", "release-drafter.yml")),
        )
}
