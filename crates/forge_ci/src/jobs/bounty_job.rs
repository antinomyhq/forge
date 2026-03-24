//! Jobs for the bounty management workflow (v2).
//!
//! v2 uses a state-reconciliation model: each job fetches the full current
//! state of an issue or PR from GitHub, computes the desired label set from
//! the rules engine, diffs current vs desired, and applies the minimal patch.
//!
//! Two entry points:
//! - `sync-issue.ts --issue N` — reconciles all bounty labels on an issue.
//! - `sync-pr.ts --pr N` — propagates labels from linked issues to the PR;
//!   handles the rewarded lifecycle on merge.

use gh_workflow::*;

const SCRIPTS_DIR: &str = ".github/scripts/bounty/src";
const TSX: &str = "npx tsx";

/// Returns a checkout step — required before script invocation.
fn checkout_step() -> Step<Use> {
    Step::new("Checkout").uses("actions", "checkout", "v4")
}

/// Builds a three-step job: checkout + npm install + a single script
/// invocation.
fn sync_job(job_name: &str, script: &str, args: String) -> Job {
    let cmd = format!("{TSX} {SCRIPTS_DIR}/{script} {args}");
    Job::new(job_name)
        .add_step(checkout_step())
        .add_step(Step::new("Install npm packages").run("npm install"))
        .add_step(Step::new("Sync bounty labels").run(cmd))
}

/// Creates a job that syncs all bounty labels on an issue.
///
/// Handles: generic `bounty` label, `bounty: claimed` on assignment.
/// Triggered on: issues assigned, unassigned, labeled, unlabeled.
pub fn sync_issue_job() -> Job {
    sync_job(
        "Sync issue bounty labels",
        "sync-issue.ts",
        "--issue ${{ github.event.issue.number }} \
            --repo ${{ github.repository }} \
            --token ${{ secrets.GITHUB_TOKEN }}"
            .to_string(),
    )
    .permissions(Permissions::default().issues(Level::Write))
}

/// Creates a job that propagates bounty labels from linked issues to the PR
/// and handles the rewarded lifecycle when the PR is merged.
///
/// Triggered on: pull_request opened/edited/reopened, pull_request_target
/// closed.
pub fn sync_pr_job() -> Job {
    sync_job(
        "Sync PR bounty labels",
        "sync-pr.ts",
        "--pr ${{ github.event.pull_request.number }} \
            --repo ${{ github.repository }} \
            --token ${{ secrets.GITHUB_TOKEN }}"
            .to_string(),
    )
    .permissions(
        Permissions::default()
            .issues(Level::Write)
            .pull_requests(Level::Write),
    )
}
