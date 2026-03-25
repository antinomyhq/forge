use gh_workflow::generate::Generate;
use gh_workflow::*;

use crate::jobs::{sync_all_issues_job, sync_pr_job};

/// Generate the bounty management workflow (v2).
///
/// Two jobs cover the full bounty lifecycle:
/// - `sync-all-issues`: fetches all open issues with any bounty label and
///   reconciles their label sets in one pass. Triggered on label/assignment
///   events and daily on a schedule.
/// - `sync-pr`: propagates bounty value labels from linked issues to the PR on
///   open/edit, and applies the rewarded lifecycle on merge.
pub fn generate_bounty_workflow() {
    let events = Event::default()
        .pull_request(
            PullRequest::default()
                .add_type(PullRequestType::Opened)
                .add_type(PullRequestType::Edited)
                .add_type(PullRequestType::Reopened),
        )
        .pull_request_target(PullRequestTarget::default().add_type(PullRequestType::Closed))
        .issues(
            Issues::default()
                .add_type(IssuesType::Assigned)
                .add_type(IssuesType::Unassigned)
                .add_type(IssuesType::Labeled)
                .add_type(IssuesType::Unlabeled),
        )
        .add_cron_schedule("0 2 * * *");

    let workflow = Workflow::default()
        .name("Bounty Management")
        .on(events)
        .permissions(
            Permissions::default()
                .issues(Level::Write)
                .pull_requests(Level::Write),
        )
        .add_job("sync-all-issues", sync_all_issues_job())
        .add_job("sync-pr", sync_pr_job());

    Generate::new(workflow)
        .name("bounty.yml")
        .generate()
        .unwrap();
}
