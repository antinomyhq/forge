use gh_workflow::generate::Generate;
use gh_workflow::*;

use crate::jobs::{mark_bounty_rewarded_job, propagate_bounty_label_job, sync_claimed_label_job};

/// Generate the bounty management workflow.
///
/// Produces three jobs:
/// - `propagate-bounty-label`: copies the bounty USD value label from a linked
///   issue to the PR when a PR is opened or edited.
/// - `sync-claimed-label`: adds `bounty: claimed` to an issue when it is
///   assigned, and removes it when all assignees are removed.
/// - `mark-bounty-rewarded`: applies `bounty: rewarded` to the merged PR and
///   its linked issues, and removes `bounty: claimed` from those issues.
pub fn generate_bounty_workflow() {
    let events = Event::default()
        .pull_request(
            PullRequest::default()
                .add_type(PullRequestType::Opened)
                .add_type(PullRequestType::Edited)
                .add_type(PullRequestType::Reopened),
        )
        .pull_request_target(
            PullRequestTarget::default().add_type(PullRequestType::Closed),
        )
        .issues(
            Issues::default()
                .add_type(IssuesType::Assigned)
                .add_type(IssuesType::Unassigned),
        );

    let workflow = Workflow::default()
        .name("Bounty Management")
        .on(events)
        .permissions(
            Permissions::default()
                .issues(Level::Write)
                .pull_requests(Level::Write),
        )
        .add_job("propagate-bounty-label", propagate_bounty_label_job())
        .add_job("sync-claimed-label", sync_claimed_label_job())
        .add_job("mark-bounty-rewarded", mark_bounty_rewarded_job());

    Generate::new(workflow)
        .name("bounty.yml")
        .generate()
        .unwrap();
}
