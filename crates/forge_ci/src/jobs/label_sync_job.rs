use gh_workflow_tailcall::*;

/// Create a job to sync GitHub labels
pub fn label_sync_job() -> Job {
    Job::new("label-sync")
        .runs_on("ubuntu-latest")
        .permissions(
            Permissions::default()
                .issues(Level::Write)
        )
        .add_step(
            Step::uses("actions", "checkout", "v5")
                .name("Checkout")
        )
        .add_step(
            Step::run(
                "npx github-label-sync \\\n  --access-token ${{ secrets.GITHUB_TOKEN }} \\\n  --labels \".github/labels.json\" \\\n  ${{ github.repository }}"
            )
                .name("Sync labels")
        )
}
