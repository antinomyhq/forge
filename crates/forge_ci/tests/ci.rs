use forge_ci::workflow;

#[test]
fn generate() {
    workflow::generate_ci_workflow();
}

#[test]
fn test_release_drafter() {
    workflow::generate_release_drafter_workflow();
}

#[test]
fn test_homebrew_workflow() {
    workflow::generate_homebrew_workflow();
}

#[test]
fn test_npm_workflow() {
    workflow::generate_npm_workflow();
}
#[test]
fn test_labels_workflow() {
    workflow::generate_labels_workflow();
}
