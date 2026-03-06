use gh_workflow::generate::Generate;
use gh_workflow::*;

/// Generate workflow triggered when the forge PR creation label is added
pub fn generate_pull_request_create_workflow() {
    let workflow = Workflow::default()
        .name("Forge Pull Request Create")
        .on(Event {
            issues: Some(Issues::default().add_type(IssuesType::Labeled)),
            ..Event::default()
        })
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .issues(Level::Read)
                .pull_requests(Level::Write),
        )
        .add_env(("OPENROUTER_API_KEY", "${{secrets.OPENROUTER_API_KEY}}"))
        .add_job(
            "pull_request_create",
            Job::new("pull-request-create")
                .cond(Expression::new(
                    "github.event.action == 'labeled' && github.event.label.name == 'forge: pull-request-create'",
                ))
                .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
                .add_step(Step::new("Install Forge").run(
                    r#"curl -fsSL https://forgecode.dev/cli | sh
echo "$HOME/.local/bin" >> "$GITHUB_PATH"
echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
forge --version"#,
                ))
                .add_step(
                    Step::new("Get GitHub Issue Details")
                        .run(
                            r#"gh issue view "${{ github.event.issue.number }}" \
  --json number,title,body,url \
  --jq '"Issue #\(.number): \(.title)\nURL: \(.url)\n\n\(.body // "")"' \
  > .github/forge-issue.md"#,
                        )
                        .add_env(("GH_TOKEN", "${{ secrets.GITHUB_TOKEN }}")),
                )
                .add_step(Step::new("Pipe Issue Content To Forge").run(
                    r#"{
  cat .github/forge-issue.md
  echo ""
  echo "Implement the issue above in this repository by applying the required code changes."
} | forge"#,
                ))
                .add_step(Step::new("Commit Changes").run(
                    r#"git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git checkout -B "forge/issue-${{ github.event.issue.number }}-pr-create"
git add .
forge commit"#,
                ))
                .add_step(
                    Step::new("Create Pull Request")
                        .run(
                            r#"git push --set-upstream origin "forge/issue-${{ github.event.issue.number }}-pr-create"
gh pr create --fill"#,
                        )
                        .add_env(("GH_TOKEN", "${{ secrets.GITHUB_TOKEN }}")),
                )
                .add_step(
                    Step::new("Generate PR Description")
                        .run("forge command execute github-pr-description")
                        .add_env(("GH_TOKEN", "${{ secrets.GITHUB_TOKEN }}")),
                ),
        );

    Generate::new(workflow)
        .name("pull-request-create.yml")
        .generate()
        .unwrap();
}
