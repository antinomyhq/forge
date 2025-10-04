use gh_workflow::generate::Generate;
use gh_workflow::*;

use crate::jobs::draft_release_update_job;

/// Generate release drafter workflow
pub fn generate_release_drafter_workflow() {
    let release_drafter = Workflow::default()
        .name("Release Drafter")
        .on(Event {
            push: Some(Push { branches: vec!["main".to_string()], ..Push::default() }),
            pull_request_target: Some(PullRequestTarget {
                types: vec![
                    PullRequestType::Opened,
                    PullRequestType::Reopened,
                    PullRequestType::Synchronize,
                    PullRequestType::Labeled,
                    PullRequestType::Unlabeled,
                    PullRequestType::Closed,
                ],
                branches: vec!["main".to_string()],
            }),
            ..Event::default()
        })
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write),
        )
        .add_job("update_release_draft", draft_release_update_job());

    Generate::new(release_drafter)
        .name("release-drafter.yml")
        .generate()
        .unwrap();
}
