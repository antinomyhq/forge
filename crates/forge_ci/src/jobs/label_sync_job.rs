use gh_workflow::*;

/// Create a job to sync GitHub labels
pub fn label_sync_job() -> Job {
    Job::new("label-sync")
        .permissions(
            Permissions::default()
                .issues(Level::Write)
        )
        .add_step(
            Step::new("Checkout Code").uses("actions", "checkout", "v6")
                .name("Checkout")
        )
        .add_step(
            Step::new("Sync Labels").run(
                "npx github-label-sync \\\n  --access-token ${{ secrets.GITHUB_TOKEN }} \\\n  --labels \".github/labels.json\" \\\n  ${{ github.repository }}"
            )
                .name("Sync labels")
        )
}
