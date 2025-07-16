use gh_workflow_tailcall::generate::Generate;
use gh_workflow_tailcall::*;

use crate::jobs::label_sync_job;

/// Generate labels workflow
pub fn generate_labels_workflow() {
    let labels_workflow = Workflow::default()
        .name("Github Label Sync")
        .on(Event {
            push: Some(Push { branches: vec!["main".to_string()], ..Push::default() }),
            ..Event::default()
        })
        .permissions(Permissions::default().contents(Level::Write))
        .add_job("label-sync", label_sync_job());

    Generate::new(labels_workflow)
        .name("labels.yml")
        .generate()
        .unwrap();
}
