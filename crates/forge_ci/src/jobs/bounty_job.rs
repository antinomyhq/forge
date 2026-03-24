//! Jobs for the bounty management workflow (v2).
//!
//! v2 uses a state-reconciliation model: each job fetches the full current
//! state of an issue or PR from GitHub, computes the desired label set from
//! the rules engine, diffs current vs desired, and applies the minimal patch.
//!
//! Three entry points:
//! - `sync-all-issues.ts` — fetches all open issues with any bounty label and
//!   reconciles their label sets in one pass. Runs on a schedule and on label
//!   events.
//! - `sync-all-prs.ts` — fetches all open PRs with any bounty label, resolves
//!   linked issues, and applies the full PR rules in one pass. Runs on a
//!   schedule and on pull_request/pull_request_target events.

use gh_workflow::*;

const SCRIPTS_DIR: &str = ".github/scripts/bounty/src";
const TSX: &str = "npx tsx";

/// Returns a checkout step — required before script invocation.
fn checkout_step() -> Step<Use> {
    Step::new("Checkout").uses("actions", "checkout", "v4")
}

/// Creates a job that syncs bounty labels across all open issues that carry
/// any bounty label.
///
/// Fetches every open issue with a "bounty" label prefix, computes the desired
/// state for each, and applies the minimal patch in a single pass.
///
/// Triggered on: issues labeled/unlabeled/assigned/unassigned, and on schedule.
pub fn sync_all_issues_job() -> Job {
    let cmd = format!(
        "{TSX} {SCRIPTS_DIR}/sync-all-issues.ts \
            --repo ${{{{ github.repository }}}} \
            --token ${{{{ secrets.GITHUB_TOKEN }}}} \
            --execute"
    );
    Job::new("Sync all bounty issues")
        .add_step(checkout_step())
        .add_step(Step::new("Install npm packages").run("npm install"))
        .add_step(Step::new("Sync all bounty labels").run(cmd))
        .permissions(Permissions::default().issues(Level::Write))
        .cond(Expression::new(
            "github.event_name == 'issues' || github.event_name == 'schedule'",
        ))
}

/// Creates a job that syncs bounty labels across all open PRs that carry any
/// bounty label, propagating value labels from linked issues and applying the
/// rewarded lifecycle on merge.
///
/// Triggered on: pull_request opened/edited/reopened, pull_request_target
/// closed, and on schedule.
pub fn sync_all_prs_job() -> Job {
    let cmd = format!(
        "{TSX} {SCRIPTS_DIR}/sync-all-prs.ts \
            --repo ${{{{ github.repository }}}} \
            --token ${{{{ secrets.GITHUB_TOKEN }}}} \
            --execute"
    );
    Job::new("Sync all bounty PRs")
        .add_step(checkout_step())
        .add_step(Step::new("Install npm packages").run("npm install"))
        .add_step(Step::new("Sync all bounty PR labels").run(cmd))
        .permissions(
            Permissions::default()
                .issues(Level::Write)
                .pull_requests(Level::Write),
        )
        .cond(Expression::new(
            "github.event_name == 'pull_request' || github.event_name == 'pull_request_target' || github.event_name == 'schedule'",
        ))
}
