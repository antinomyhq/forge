use forge_ci::{jobs, workflow};

#[test]
fn generate() {
    workflow::generate_ci_workflow();
}

#[test]
fn test_apt_get_install() {
    let packages = &["pkg1", "pkg2", "pkg3"];
    let command = jobs::apt_get_install(packages);
    assert_eq!(
        command,
        "sudo apt-get update && \\\nsudo apt-get install -y \\\n  pkg1 \\\n  pkg2 \\\n  pkg3"
    );
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
