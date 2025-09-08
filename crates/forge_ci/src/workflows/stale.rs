use gh_workflow_tailcall::generate::Generate;
use gh_workflow_tailcall::*;
use indexmap::indexmap;
use serde_json::json;

/// Generate the stale issues and PRs workflow
pub fn generate_stale_workflow() {
    let workflow = Workflow::default()
        .name("Close Stale Issues and PR")
        .on(Event {
            schedule: Some(Schedule {
                cron: vec!["0 * * * *"], // This runs every hour
            }),
            workflow_dispatch: Some(WorkflowDispatch::default()),
            ..Event::default()
        })
        .permissions(
            Permissions::default()
                .issues(Level::Write)
                .pull_requests(Level::Write),
        )
        .env(Env::from(indexmap! {
            "DAYS_BEFORE_ISSUE_STALE" => json!("30"),
            "DAYS_BEFORE_ISSUE_CLOSE" => json!("7"),
            "DAYS_BEFORE_PR_STALE" => json!("5"),
            "DAYS_BEFORE_PR_CLOSE" => json!("10"),
        }))
        .add_job(
            "stale",
            Job::default()
                .runs_on("ubuntu-latest")
                .add_step(
                    Step::uses("actions", "stale", "v9")
                        .with(Input::from(indexmap! {
                            "stale-issue-label" => json!("state: inactive"),
                            "stale-pr-label" => json!("state: inactive"),
                            "stale-issue-message" => json!(r#"**Action required:** Issue inactive for ${{ env.DAYS_BEFORE_ISSUE_STALE }} days.
Status update or closure in ${{ env.DAYS_BEFORE_ISSUE_CLOSE }} days."#),
                            "close-issue-message" => json!("Issue closed after ${{ env.DAYS_BEFORE_ISSUE_CLOSE }} days of inactivity."),
                            "stale-pr-message" => json!(r#"**Action required:** PR inactive for ${{ env.DAYS_BEFORE_PR_STALE }} days.
Status update or closure in ${{ env.DAYS_BEFORE_PR_CLOSE }} days."#),
                            "close-pr-message" => json!("PR closed after ${{ env.DAYS_BEFORE_PR_CLOSE }} days of inactivity."),
                            "days-before-issue-stale" => json!("${{ env.DAYS_BEFORE_ISSUE_STALE }}"),
                            "days-before-issue-close" => json!("${{ env.DAYS_BEFORE_ISSUE_CLOSE }}"),
                            "days-before-pr-stale" => json!("${{ env.DAYS_BEFORE_PR_STALE }}"),
                            "days-before-pr-close" => json!("${{ env.DAYS_BEFORE_PR_CLOSE }}"),
                        })),
                ),
        );

    Generate::new(workflow)
        .name("stale.yml")
        .generate()
        .unwrap();
}
