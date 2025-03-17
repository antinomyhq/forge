use generate::Generate;
use gh_workflow_tailcall::*;
use indexmap::indexmap;
use serde_json::json;

/// Helper function to generate an apt-get install command for multiple packages
///
/// # Examples
/// ```
/// let command = apt_get_install(&["pkg1", "pkg2"]);
/// assert_eq!(command, "sudo apt-get update && \\\nsudo apt-get install -y \\\n  pkg1 \\\n  pkg2");
/// ```
fn apt_get_install(packages: &[&str]) -> String {
    format!(
        "sudo apt-get update && \\\nsudo apt-get install -y \\\n{}",
        packages
            .iter()
            .map(|pkg| format!("  {}", pkg))
            .collect::<Vec<_>>()
            .join(" \\\n")
    )
}

#[test]
fn generate() {
    let mut workflow = StandardWorkflow::default()
        .auto_fix(true)
        .to_ci_workflow()
        .concurrency(Concurrency {
            group: "${{ github.workflow }}-${{ github.ref }}".to_string(),
            cancel_in_progress: None,
            limit: None,
        })
        .add_env(("OPENROUTER_API_KEY", "${{secrets.OPENROUTER_API_KEY}}"))
        .add_env(("RUN_API_TESTS", "0"));

    // Set up the build matrix for all platforms
    let matrix = json!({
        "include": [
            {
                "os": "ubuntu-latest",
                "target": "x86_64-unknown-linux-musl",
                "binary_name": "forge-x86_64-unknown-linux-musl",
                "binary_path": "target/x86_64-unknown-linux-musl/release/forge",
                "cross": "false"
            },
            {
                "os": "ubuntu-latest",
                "target": "aarch64-unknown-linux-musl",
                "binary_name": "forge-aarch64-unknown-linux-musl",
                "binary_path": "target/aarch64-unknown-linux-musl/release/forge",
                "cross": "false"
            },
            {
                "os": "ubuntu-latest",
                "target": "x86_64-unknown-linux-gnu",
                "binary_name": "forge-x86_64-unknown-linux-gnu",
                "binary_path": "target/x86_64-unknown-linux-gnu/release/forge",
                "cross": "false"
            },
            {
                "os": "ubuntu-latest",
                "target": "aarch64-unknown-linux-gnu",
                "binary_name": "forge-aarch64-unknown-linux-gnu",
                "binary_path": "target/aarch64-unknown-linux-gnu/release/forge",
                "cross": "true"
            },
            {
                "os": "macos-latest",
                "target": "x86_64-apple-darwin",
                "binary_name": "forge-x86_64-apple-darwin",
                "binary_path": "target/x86_64-apple-darwin/release/forge",
                "cross": "false"
            },
            {
                "os": "macos-latest",
                "target": "aarch64-apple-darwin",
                "binary_name": "forge-aarch64-apple-darwin",
                "binary_path": "target/aarch64-apple-darwin/release/forge",
                "cross": "false"
            },
            {
                "os": "windows-latest",
                "target": "x86_64-pc-windows-msvc",
                "binary_name": "forge-x86_64-pc-windows-msvc.exe",
                "binary_path": "target/x86_64-pc-windows-msvc/release/forge.exe",
                "cross": "false"
            },
            {
                "os": "windows-latest",
                "target": "aarch64-pc-windows-msvc",
                "binary_name": "forge-aarch64-pc-windows-msvc.exe",
                "binary_path": "target/aarch64-pc-windows-msvc/release/forge.exe",
                "cross": "false"
            }
        ]
    });

    let build_job = workflow.jobs.clone().unwrap().get("build").unwrap().clone();
    let main_cond =
        Expression::new("github.event_name == 'push' && github.ref == 'refs/heads/main'");

    // Add draft release job
    let draft_release_job = Job::new("draft_release")
            .runs_on("ubuntu-latest")
            .cond(main_cond.clone())
            .permissions(
                Permissions::default()
                    .contents(Level::Write)
                    .pull_requests(Level::Write),
            )
            .add_step(Step::uses("actions", "checkout", "v4"))
            .add_step(
                Step::uses("release-drafter", "release-drafter", "v6")
                    .id("create_release")
                    .env(("GITHUB_TOKEN", "${{ secrets.GITHUB_TOKEN }}"))
                    .with(("config-name", "release-drafter.yml")),
            )
            .add_step(
                Step::run("echo \"create_release_id=${{ steps.create_release.outputs.id }}\" >> $GITHUB_OUTPUT && echo \"create_release_name=${{ steps.create_release.outputs.tag_name }}\" >> $GITHUB_OUTPUT")
                    .id("set_output"),
            )
            .outputs(indexmap! {
                "create_release_name".to_string() => "${{ steps.set_output.outputs.create_release_name }}".to_string(),
                "create_release_id".to_string() => "${{ steps.set_output.outputs.create_release_id }}".to_string()
            });
    workflow = workflow.add_job("draft_release", draft_release_job.clone());
    // Build and upload release job
    // Store reference to build-release job
    let build_release_job = Job::new("build-release")
        .strategy(Strategy {
            fail_fast: None,
            max_parallel: None,
            matrix: Some(matrix.clone()),
        })
        .runs_on("${{ matrix.os }}")
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write),
        )
        .add_step(Step::uses("actions", "checkout", "v4"))
        // Install Rust with cross-compilation target
        .add_step(
            Step::uses("taiki-e", "setup-cross-toolchain-action", "v1")
                .with(("target", "${{ matrix.target }}")),
        )
        // Build add link flags
        .add_step(
            Step::run(r#"echo "RUSTFLAGS=-C target-feature=+crt-static" >> $GITHUB_ENV"#)
                .if_condition(Expression::new(
                    "!contains(matrix.target, '-unknown-linux-gnu')",
                )),
        )
        .add_step(
            Step::run(apt_get_install(&[
                "gcc-aarch64-linux-gnu",
                "musl-tools",
                "musl-dev",
                "pkg-config",
                "libssl-dev",
            ]))
            .if_condition(Expression::new(
                "contains(matrix.target, '-unknown-linux-musl')",
            )),
        )
        // Build release binary
        .add_step(
            Step::uses("ClementTsang", "cargo-action", "v0.0.6")
                .add_with(("command", "build --release"))
                .add_with(("args", "--target ${{ matrix.target }}"))
                .add_with(("use-cross", "${{ matrix.cross }}"))
                .add_with(("cross-version", "0.2.4"))
                .add_env(("RUSTFLAGS", "${{ env.RUSTFLAGS }}"))
                .add_env(("POSTHOG_API_SECRET", "${{secrets.POSTHOG_API_SECRET}}"))
                .add_env((
                    "APP_VERSION",
                    "${{ needs.draft_release.outputs.create_release_name }}",
                )),
        );
    let label_cond = Expression::new("github.event_name == 'pull_request' && contains(github.event.pull_request.labels.*.name, 'build-all-targets')");
    workflow = workflow.add_job(
        "build-release-pr",
        build_release_job.clone().cond(label_cond),
    );
    workflow = workflow.add_job(
        "build-release",
        build_release_job
            .add_needs(build_job.clone())
            .add_needs(draft_release_job.clone())
            .cond(main_cond.clone())
            // Rename binary to target name
            .add_step(Step::run(
                "cp ${{ matrix.binary_path }} ${{ matrix.binary_name }}",
            ))
            // Upload directly to release
            .add_step(
                Step::uses("xresloader", "upload-to-github-release", "v1")
                    .add_with((
                        "release_id",
                        "${{ needs.draft_release.outputs.create_release_id }}",
                    ))
                    .add_with(("file", "${{ matrix.binary_name }}"))
                    .add_with(("overwrite", "true")),
            ),
    );
    // Store reference to build-release job
    let build_release_job = workflow
        .jobs
        .clone()
        .unwrap()
        .get("build-release")
        .unwrap()
        .clone();

    // Add semantic release job to publish the release
    let semantic_release_job = Job::new("semantic_release")
            .add_needs(draft_release_job.clone())
            .add_needs(build_release_job.clone())
            .cond(Expression::new("(startsWith(github.event.head_commit.message, 'feat') || startsWith(github.event.head_commit.message, 'fix')) && (github.event_name == 'push' && github.ref == 'refs/heads/main')"))
            .permissions(
                Permissions::default()
                    .contents(Level::Write)
                    .pull_requests(Level::Write),
            )
            .runs_on("ubuntu-latest")
            .env(("GITHUB_TOKEN", "${{ secrets.GITHUB_TOKEN }}"))
            .env(("APP_VERSION", "${{ needs.draft_release.outputs.create_release_name }}"))
            .add_step(
                Step::uses("test-room-7", "action-publish-release-drafts", "v0")
                    .env(("GITHUB_TOKEN", "${{ secrets.GITHUB_TOKEN }}"))
                    .add_with(("github-token", "${{ secrets.GITHUB_TOKEN }}"))
                    .add_with(("tag-name", "${{ needs.draft_release.outputs.create_release_name }}")),
            );
    workflow = workflow.add_job("semantic_release", semantic_release_job.clone());

    // Homebrew release job
    workflow = workflow.add_job(
        "homebrew_release",
        Job::new("homebrew_release")
            .add_needs(draft_release_job.clone())
            .add_needs(build_release_job.clone())
            .add_needs(semantic_release_job.clone())
            .cond(Expression::new("(startsWith(github.event.head_commit.message, 'feat') || startsWith(github.event.head_commit.message, 'fix')) && (github.event_name == 'push' && github.ref == 'refs/heads/main')"))
            .permissions(
                Permissions::default()
                    .contents(Level::Write)
                    .pull_requests(Level::Write),
            )
            .runs_on("ubuntu-latest")
            .add_step(
                Step::uses("actions", "checkout", "v4")
                    .add_with(("repository", "antinomyhq/homebrew-code-forge"))
                    .add_with(("ref", "main"))
                    .add_with(("token", "${{ secrets.HOMEBREW_ACCESS }}")),
            )
            // Make script executable and run it with token
            .add_step(
                Step::run("GITHUB_TOKEN=\"${{ secrets.HOMEBREW_ACCESS }}\" ./update-formula.sh ${{needs.draft_release.outputs.create_release_name }}"),
            ),
    );

    workflow.generate().unwrap();
}
#[test]
fn test_apt_get_install() {
    let packages = &["pkg1", "pkg2", "pkg3"];
    let command = apt_get_install(packages);
    assert_eq!(
        command,
        "sudo apt-get update && \\\nsudo apt-get install -y \\\n  pkg1 \\\n  pkg2 \\\n  pkg3"
    );
}

#[test]
fn test_forge_automation() {
    // Generate Forge Automation workflow
    let mut forge_automation = Workflow::default()
        .name("Forge Automation")
        .on(Event {
            issues: Some(Issues { types: vec![IssuesType::Labeled] }),
            issue_comment: Some(IssueComment { types: vec![IssueCommentType::Created] }),
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

    // Process issues job - runs when an issue is labeled with "forge-just-do-it"
    forge_automation = forge_automation.add_job(
        "process_issue",
        Job::new("process_issue")
            .runs_on("ubuntu-latest")
            .cond(Expression::new("github.event_name == 'issues' && github.event.label.name == 'forge-just-do-it'"))
            .add_step(Step::uses("tibdex", "github-app-token", "v2")
            .id("generate-token")
            .add_with(("private_key", "${{ secrets.FORGE_BOT_PRIVATE_KEY }}"))
            .add_with(("app_id", "${{secrets.FORGE_BOT_APP_ID}}")))
            .add_step(Step::uses("actions", "checkout", "v4"))
            .add_step(
                Step::run("curl -L https://raw.githubusercontent.com/antinomyhq/forge/main/install.sh | bash")
                    .name("Install Forge CLI"),
            )
            .add_step(
                Step::uses("peter-evans", "create-or-update-comment", "v4")
                    .name("Add comment to issue with action link")
                    .add_with(("token", "${{ steps.generate-token.outputs.token }}"))
                    .add_with(("issue-number", "${{ github.event.issue.number }}"))
                    .add_with(("body", "✨ **Forge Automation Engaged!** ✨\n\nI've started working on this issue with the power of AI. You can watch my progress in the [GitHub Action run](https://github.com/${{ github.repository }}/actions/runs/${{ github.run_id }}).\n\nI'll analyze the issue and submit a solution shortly. Stay tuned for updates!")),
            ).add_step(
                Step::run("forge --event='{\"name\": \"fix_issue\", \"value\": \"${{ github.event.issue.number }}\"}'")
                    .name("Run Forge to process issue")
                    .add_env(("GITHUB_TOKEN", "${{ steps.generate-token.outputs.token }}"))
                    .add_env(("FORGE_KEY", "${{ secrets.FORGE_KEY }}")),
            ),
    );

    // Process PR comment job - runs when a comment is added to a PR with the
    // "forge-just-do-it" label
    forge_automation = forge_automation.add_job(
        "update_pr",
        Job::new("update_pr")
            .runs_on("ubuntu-latest")
            .cond(Expression::new(
                "(github.event_name == 'issue_comment' && \
                  github.event.issue.pull_request && \
                  contains(github.event.issue.labels.*.name, 'forge-just-do-it') && \
                  github.actor != 'forge-at-your-service[bot]') || \
                 (github.event_name == 'pull_request_review' && \
                  contains(github.event.pull_request.labels.*.name, 'forge-just-do-it') && \
                  github.actor != 'forge-at-your-service[bot]') || \
                 (github.event_name == 'pull_request_review_comment' && \
                  contains(github.event.pull_request.labels.*.name, 'forge-just-do-it') && \
                  github.actor != 'forge-at-your-service[bot]')",
            ))
            .add_step(Step::uses("tibdex", "github-app-token", "v2")
            .id("generate-token")
            .add_with(("private_key", "${{ secrets.FORGE_BOT_PRIVATE_KEY }}"))
            .add_with(("app_id", "${{secrets.FORGE_BOT_APP_ID}}"))
        )
            .add_step(Step::uses("actions", "checkout", "v4"))
            .add_step(
                Step::run("curl -L https://raw.githubusercontent.com/antinomyhq/forge/main/install.sh | bash")
                    .name("Install Forge CLI"),
            )
            .add_step(
                Step::uses("peter-evans", "create-or-update-comment", "v4")
                    .name("Add comment to PR with action link")
                    .add_with(("token", "${{ steps.generate-token.outputs.token }}"))
                    .add_with(("issue-number", "${{ github.event_name == 'issue_comment' && github.event.issue.number || github.event.pull_request.number }}"))
                    .add_with(("body", "🔧 **Forge at your service!** 🔧\n\nI'm processing your comment and updating this PR accordingly. Watch the magic happen in the [GitHub Action run](https://github.com/${{ github.repository }}/actions/runs/${{ github.run_id }}).\n\nI'll analyze your request and implement the suggested changes. Check back soon for updates!")),
            )
            .add_step(
                Step::run("forge --event='{\"name\": \"update_pr\", \"value\": \"${{ github.event_name == 'issue_comment' && github.event.issue.number || github.event.pull_request.number }}\"}'") 
                    .name("Run Forge to update PR based on comment")
                    .add_env(("GITHUB_TOKEN", "${{ steps.generate-token.outputs.token }}"))
                    .add_env(("FORGE_KEY", "${{ secrets.FORGE_KEY }}"))
            )
            ,
    );

    Generate::new(forge_automation)
        .name("forge-automation.yml")
        .generate()
        .unwrap();
}

#[test]
fn test_release_drafter() {
    // Generate Release Drafter workflow
    let mut release_drafter = Workflow::default()
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

    release_drafter = release_drafter.add_job(
        "update_release_draft",
        Job::new("update_release_draft")
            .runs_on("ubuntu-latest")
            .add_step(
                Step::uses("release-drafter", "release-drafter", "v6")
                    .env(("GITHUB_TOKEN", "${{ secrets.GITHUB_TOKEN }}"))
                    .add_with(("config-name", "release-drafter.yml")),
            ),
    );

    release_drafter = release_drafter.name("Release Drafter");
    Generate::new(release_drafter)
        .name("release-drafter.yml")
        .generate()
        .unwrap();
}
