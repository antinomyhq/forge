use generate::Generate;
use gh_workflow_tailcall::*;

/// Generate a comment body with a title, action link, and custom message
fn generate_comment_body(emoji: &str, title: &str, message: &str) -> String {
    format!(
        "{} **{}** {}\n\n{} You can track progress in the [GitHub Action run](https://github.com/${{{{ github.repository }}}}/actions/runs/${{{{ github.run_id }}}}).\n\n{}",
        emoji, title, emoji, message,
        if message.ends_with(".") { "" } else { "." }
    )
}

/// Generate a forge event JSON string with proper escaping
fn forge_event_json(event_name: &str, value_expr: &str) -> String {
    let escaped_value = value_expr
        .replace('\\', "\\\\") // Escape backslashes first
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t");

    format!(
        "forge --event='{{\"name\": \"{}\", \"value\": \"{}\"}}'",
        event_name, escaped_value
    )
}

/// Get the appropriate issue/PR number based on event context
fn get_issue_or_pr_number() -> String {
    "${{ github.event_name == 'issue_comment' && github.event.issue.number || github.event.pull_request.number }}".to_string()
}

/// Add common steps for token generation and checkout
fn add_common_setup_steps(job: Job) -> Job {
    job.add_step(
        Step::uses("tibdex", "github-app-token", "v2")
            .id("generate-token")
            .add_with(("private_key", "${{ secrets.FORGE_BOT_PRIVATE_KEY }}"))
            .add_with(("app_id", "${{secrets.FORGE_BOT_APP_ID}}")),
    )
    .add_step(Step::uses("actions", "checkout", "v4"))
}

/// Add forge CLI installation step
fn add_forge_cli_installation(job: Job) -> Job {
    job.add_step(
        Step::run(
            "curl -L https://raw.githubusercontent.com/antinomyhq/forge/main/install.sh | bash",
        )
        .name("Install Forge CLI"),
    )
}

/// Generate condition for issue_comment with label and comment prefix
fn issue_comment_condition(label: &str, comment_prefix: &str) -> String {
    format!(
        "github.event_name == 'issue_comment' && github.event.issue.pull_request && contains(github.event.issue.labels.*.name, '{}') && startsWith(github.event.comment.body, '{}')",
        label, comment_prefix
    )
}

/// Add PR branch checkout steps with dynamic PR number
fn add_pr_checkout_steps(job: Job, pr_number_expr: &str) -> Job {
    job.add_step(
        Step::run(format!(
            "git fetch origin pull/{}/head:pr-{}",
            pr_number_expr, pr_number_expr
        ))
        .name("Fetch PR branch"),
    )
    .add_step(Step::run(format!("git checkout pr-{}", pr_number_expr)).name("Checkout PR branch"))
}

#[test]
fn test_forge_automation() {
    // Generate Forge Automation workflow
    let mut forge_automation = Workflow::default()
        .name("Forge Automation")
        .on(Event {
            issues: Some(Issues { types: vec![IssuesType::Labeled] }),
            issue_comment: Some(IssueComment { types: vec![IssueCommentType::Created] }),
            pull_request: Some(PullRequest {
                types: vec![
                    PullRequestType::Opened,
                    PullRequestType::Reopened,
                    PullRequestType::Labeled,
                    PullRequestType::Unlabeled,
                    PullRequestType::Edited,
                ],
                branches: vec![],
                paths: vec![],
            }),
            pull_request_review: Some(PullRequestReview {
                types: vec![
                    PullRequestReviewType::Submitted,
                    PullRequestReviewType::Edited,
                ],
            }),
            pull_request_review_comment: Some(PullRequestReviewComment {
                types: vec![
                    PullRequestReviewCommentType::Created,
                    PullRequestReviewCommentType::Edited,
                ],
            }),
            ..Event::default()
        })
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .issues(Level::Write)
                .pull_requests(Level::Write),
        );

    // Process issues job - runs when an issue is labeled with "forge-automate"
    let process_issue_job = add_forge_cli_installation(
        add_common_setup_steps(
            Job::new("process_issue")
                .runs_on("ubuntu-latest")
                .cond(Expression::new("github.event_name == 'issues' && github.event.label.name == 'forge-automate'"))
        ))
        .add_step(
            Step::uses("peter-evans", "create-or-update-comment", "v4")
                .name("Add comment to issue with action link")
                .add_with(("token", "${{ steps.generate-token.outputs.token }}"))
                .add_with(("issue-number", "${{ github.event.issue.number }}"))
                .add_with(("body", generate_comment_body(
                    "✨", 
                    "Forge Automation Engaged!", 
                    "I've started working on this issue with the power of AI. I'll analyze the issue and create a plan for review. Stay tuned for updates"
                ))),
        )
        .add_step(
            Step::run(forge_event_json("fix_issue", "${{ github.event.issue.number }}"))
                .name("Run Forge to process issue")
                .add_env(("GITHUB_TOKEN", "${{ steps.generate-token.outputs.token }}"))
                .add_env(("FORGE_KEY", "${{ secrets.FORGE_KEY }}")),
        );

    forge_automation = forge_automation.add_job("process_issue", process_issue_job);

    // Revise plan job - runs when a comment with "/forge revise-plan" is added to a
    // PR with the "forge-plan-review" label
    let revise_plan_job = add_forge_cli_installation(
        add_pr_checkout_steps(
            add_common_setup_steps(
                Job::new("revise_plan")
                    .runs_on("ubuntu-latest")
                    .cond(Expression::new(
                        issue_comment_condition("forge-plan-review", "/forge revise-plan")
                    ))
            ),
            "${{ github.event.issue.number }}"
        ))
        .add_step(
            Step::uses("peter-evans", "create-or-update-comment", "v4")
                .name("Add comment to PR about plan revision")
                .add_with(("token", "${{ steps.generate-token.outputs.token }}"))
                .add_with(("issue-number", "${{ github.event.issue.number }}"))
                .add_with(("body", generate_comment_body(
                    "📝", 
                    "Revising Plan", 
                    "I'm working on revising the plan based on your feedback. I'll update the task file with the revised plan shortly"
                ))),
        )
        .add_step(
            Step::run(forge_event_json("revise_plan", "${{ github.event.issue.number }}"))
                .name("Run Forge to revise plan based on feedback")
                .add_env(("GITHUB_TOKEN", "${{ steps.generate-token.outputs.token }}"))
                .add_env(("FORGE_KEY", "${{ secrets.FORGE_KEY }}")),
        );

    forge_automation = forge_automation.add_job("revise_plan", revise_plan_job);

    // Approve plan job - runs when a comment with "/forge approve-plan" is added to
    // a PR with the "forge-plan-review" label
    let approve_plan_job = add_common_setup_steps(
        Job::new("approve_plan")
            .runs_on("ubuntu-latest")
            .cond(Expression::new(
                issue_comment_condition("forge-plan-review", "/forge approve-plan")
            ))
    )
    .add_step(
        Step::uses("peter-evans", "create-or-update-comment", "v4")
            .name("Add comment to PR about plan approval")
            .add_with(("token", "${{ steps.generate-token.outputs.token }}"))
            .add_with(("issue-number", "${{ github.event.issue.number }}"))
            .add_with(("body", generate_comment_body(
                "✅", 
                "Plan Approved", 
                "Thank you for approving the plan! I'm now ready to implement the changes. You can either wait for automatic implementation or trigger it manually with `/forge continue`"
            ))),
    )
    .add_step(
        Step::uses("actions", "github-script", "v6")
            .name("Update labels: remove forge-plan-review, add forge-implement")
            .add_with(("script", "\nconst issueNumber = context.payload.issue.number;\n\n// Remove forge-plan-review label\nawait github.rest.issues.removeLabel({\n  owner: context.repo.owner,\n  repo: context.repo.repo,\n  issue_number: issueNumber,\n  name: 'forge-plan-review'\n});\n\n// Add forge-implement label\nawait github.rest.issues.addLabels({\n  owner: context.repo.owner,\n  repo: context.repo.repo,\n  issue_number: issueNumber,\n  labels: ['forge-implement']\n});\n"))
            .add_env(("GITHUB_TOKEN", "${{ steps.generate-token.outputs.token }}")),
    );

    forge_automation = forge_automation.add_job("approve_plan", approve_plan_job);

    // Implement PR job - runs when a PR has the "forge-implement" label and not the
    // "forge-plan-review" label, or when a comment with "/forge continue" is
    // added to a PR with the "forge-implement" label
    let implement_pr_condition = "(github.event_name == 'pull_request' && contains(github.event.pull_request.labels.*.name, 'forge-implement') && !contains(github.event.pull_request.labels.*.name, 'forge-plan-review') && github.event.pull_request.draft == true) || (github.event_name == 'issue_comment' && github.event.issue.pull_request && contains(github.event.issue.labels.*.name, 'forge-implement') && !contains(github.event.issue.labels.*.name, 'forge-plan-review') && startsWith(github.event.comment.body, '/forge continue'))";

    let pr_number = get_issue_or_pr_number();

    let implement_pr_job = add_forge_cli_installation(
        add_pr_checkout_steps(
            add_common_setup_steps(
                Job::new("implement_pr")
                    .runs_on("ubuntu-latest")
                    .cond(Expression::new(implement_pr_condition))
            ),
            &pr_number
        ))
        .add_step(
            Step::uses("peter-evans", "create-or-update-comment", "v4")
                .name("Add comment to PR about implementation")
                .add_with(("token", "${{ steps.generate-token.outputs.token }}"))
                .add_with(("issue-number", &pr_number))
                .add_with(("body", generate_comment_body(
                    "🔨️", 
                    "Implementation In Progress", 
                    "I'm now implementing the approved plan. I'll update this PR with the implementation soon"
                ))),
        )
        .add_step(
            Step::run(forge_event_json("update_pr", &pr_number))
                .name("Run Forge to implement PR based on approved plan")
                .add_env(("GITHUB_TOKEN", "${{ steps.generate-token.outputs.token }}"))
                .add_env(("FORGE_KEY", "${{ secrets.FORGE_KEY }}")),
        );

    forge_automation = forge_automation.add_job("implement_pr", implement_pr_job);

    // Handle review comments job - runs when a review comment is added to a PR with
    // the "forge-automate" label
    let handle_review_condition = "(github.event_name == 'pull_request_review_comment' || github.event_name == 'pull_request_review') && (contains(github.event.pull_request.labels.*.name, 'forge-automate') || contains(github.event.pull_request.labels.*.name, 'forge-implement'))";

    let handle_review_job = add_forge_cli_installation(
        add_pr_checkout_steps(
            add_common_setup_steps(
                Job::new("handle_review")
                    .runs_on("ubuntu-latest")
                    .cond(Expression::new(handle_review_condition))
            ),
            "${{ github.event.pull_request.number }}"
        ))
        .add_step(
            Step::uses("peter-evans", "create-or-update-comment", "v4")
                .name("Add comment to PR about handling review comment")
                .add_with(("token", "${{ steps.generate-token.outputs.token }}"))
                .add_with(("issue-number", "${{ github.event.pull_request.number }}"))
                .add_with(("body", generate_comment_body(
                    "💬", 
                    "Processing Review Comment", 
                    "I'm analyzing and addressing this review comment. I'll update the PR shortly with the requested changes"
                ))),
        )
        .add_step(
            Step::run(forge_event_json("fix-review-comment", "${{ github.event.pull_request.number }}"))
                .name("Run Forge to address review comments")
                .add_env(("GITHUB_TOKEN", "${{ steps.generate-token.outputs.token }}"))
                .add_env(("FORGE_KEY", "${{ secrets.FORGE_KEY }}")),
        );

    forge_automation = forge_automation.add_job("handle_review", handle_review_job);

    // Handle regular PR comments job - runs when a regular comment is added to a PR
    // with the "forge-automate" or "forge-implement" label, excluding comments
    // starting with "/forge" and excluding comments made by the bot itself to
    // prevent infinite loops
    let handle_pr_comment_condition = "github.event_name == 'issue_comment' && github.event.issue.pull_request && (contains(github.event.issue.labels.*.name, 'forge-automate') || contains(github.event.issue.labels.*.name, 'forge-implement')) && !startsWith(github.event.comment.body, '/forge') && github.actor != 'forge-by-antinomy[bot]'";

    let handle_pr_comment_job = add_forge_cli_installation(
        add_pr_checkout_steps(
            add_common_setup_steps(
                Job::new("handle_pr_comment")
                    .runs_on("ubuntu-latest")
                    .cond(Expression::new(handle_pr_comment_condition))
            ),
            "${{ github.event.issue.number }}"
        ))
        .add_step(
            Step::uses("peter-evans", "create-or-update-comment", "v4")
                .name("Add comment to PR about handling general comment")
                .add_with(("token", "${{ steps.generate-token.outputs.token }}"))
                .add_with(("issue-number", "${{ github.event.issue.number }}"))
                .add_with(("body", generate_comment_body(
                    "💬", 
                    "Processing PR Comment", 
                    "I'm analyzing and addressing your comment. I'll update the PR shortly with any needed changes"
                ))),
        )
        .add_step(
            Step::run(forge_event_json("fix-review-comment", "${{ github.event.issue.number }}"))
                .name("Run Forge to address PR comment")
                .add_env(("GITHUB_TOKEN", "${{ steps.generate-token.outputs.token }}"))
                .add_env(("FORGE_KEY", "${{ secrets.FORGE_KEY }}")),
        );

    forge_automation = forge_automation.add_job("handle_pr_comment", handle_pr_comment_job);

    Generate::new(forge_automation)
        .name("forge-automation.yml")
        .generate()
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_comment_body() {
        let body = generate_comment_body("⭐", "Test Title", "This is a test message");
        assert!(body.contains("⭐ **Test Title** ⭐"));
        assert!(body.contains("This is a test message"));
        assert!(body.contains("[GitHub Action run]"));
    }

    #[test]
    fn test_forge_event_json() {
        let json = forge_event_json("test_event", "test_value");
        assert_eq!(
            json,
            "forge --event='{\"name\": \"test_event\", \"value\": \"test_value\"}'"
        );
    }

    #[test]
    fn test_issue_comment_condition() {
        let condition = issue_comment_condition("test-label", "/test-command");
        assert!(condition.contains("test-label"));
        assert!(condition.contains("/test-command"));
    }

    #[test]
    fn test_pr_comment_condition() {
        let condition = "github.event_name == 'issue_comment' && github.event.issue.pull_request && (contains(github.event.issue.labels.*.name, 'forge-automate') || contains(github.event.issue.labels.*.name, 'forge-implement')) && !startsWith(github.event.comment.body, '/forge') && github.actor != 'forge-by-antinomy[bot]'";
        assert!(condition.contains("github.event_name == 'issue_comment'"));
        assert!(condition.contains("github.event.issue.pull_request"));
        assert!(condition.contains("contains(github.event.issue.labels.*.name, 'forge-automate')"));
        assert!(condition.contains("!startsWith(github.event.comment.body, '/forge')"));
        assert!(condition.contains("github.actor != 'forge-by-antinomy[bot]'"));
    }

    #[test]
    fn test_forge_event_json_with_special_chars() {
        let json = forge_event_json("test_event", "test_value\r\nwith\tspecial\"chars");
        assert_eq!(
            json,
            "forge --event='{\"name\": \"test_event\", \"value\": \"test_value\\r\\nwith\\tspecial\\\"chars\"}'"
        );
    }
}
