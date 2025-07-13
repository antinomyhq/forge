use gh_workflow_tailcall::*;

/// Create a GitHub Label Sync workflow
pub fn create_labels_workflow() -> Workflow {
    let mut labels_workflow = Workflow::default()
        .name("Github Label Sync")
        .on(Event {
            push: Some(Push { branches: vec!["main".to_string()], ..Push::default() }),
            ..Event::default()
        })
        .permissions(Permissions::default().contents(Level::Write));

    labels_workflow = labels_workflow.add_job("label-sync", create_label_sync_job());

    labels_workflow
}

/// Create a job to sync GitHub labels
pub fn create_label_sync_job() -> Job {
    Job::new("label-sync")
        .runs_on("ubuntu-latest")
        .permissions(
            Permissions::default()
                .issues(Level::Write)
        )
        .add_step(
            Step::uses("actions", "checkout", "v4")
                .name("Checkout")
        )
        .add_step(
            Step::run(
                "npx github-label-sync \\\n  --access-token ${{ secrets.GITHUB_TOKEN }} \\\n  --labels \".github/labels.json\" \\\n  ${{ github.repository }}"
            )
                .name("Sync labels")
        )
}
