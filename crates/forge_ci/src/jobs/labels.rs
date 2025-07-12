use gh_workflow_tailcall::*;

/// Create a GitHub Label Sync workflow
pub fn create_labels_workflow() -> Workflow {
    let mut labels_workflow = Workflow::default().name("Github Label Sync").on(Event {
        push: Some(Push {
            branches: vec!["main".to_string()],
            paths: vec![".github/labels.json".to_string()],
            ..Push::default()
        }),
        ..Event::default()
    });

    labels_workflow = labels_workflow.add_job("label-sync", create_label_sync_job());

    labels_workflow
}

/// Create a job to sync GitHub labels
pub fn create_label_sync_job() -> Job {
    Job::new("label-sync")
        .runs_on("ubuntu-latest")
        .add_step(
            Step::uses("actions", "checkout", "v4")
                .name("Checkout")
        )
        .add_step(
            Step::run("sudo npm install --global github-label-sync")
                .name("Install github-label-sync")
        )
        .add_step(
            Step::run(
                "github-label-sync \\\n  --access-token ${{ secrets.GITHUB_TOKEN }} \\\n  --labels \".github/labels.json\" \\\n  --allow-added-labels \\\n  ${{ github.repository }}"
            )
            .name("Sync labels")
        )
}
