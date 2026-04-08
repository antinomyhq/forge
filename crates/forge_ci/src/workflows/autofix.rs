use gh_workflow::generate::Generate;
use gh_workflow::toolchain::Component;
use gh_workflow::*;

use crate::jobs;
use crate::steps::setup_protoc;

/// Generate the autofix workflow
pub fn generate_autofix_workflow() {
    let lint_fix_job = Job::new("Lint Fix")
        .permissions(Permissions::default().contents(Level::Write))
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
        .add_step(Step::new("Install SQLite").run("sudo apt-get install -y libsqlite3-dev"))
        .add_step(setup_protoc())
        .add_step(
            Step::toolchain()
                .add_nightly()
                .add_component(Component::Clippy)
                .add_component(Component::Rustfmt),
        )
        .add_step(Step::new("Cargo Fmt").run(jobs::fmt_cmd(true)))
        .add_step(Step::new("Cargo Clippy").run(jobs::clippy_cmd(true)))
        .add_step(
            Step::new("Commit and push fixes")
                .run(r#"git config user.name "github-actions[bot]"
git config user.email "github-actions[bot]@users.noreply.github.com"
git add -A
git diff --staged --quiet || git commit -m "style: auto-fix lint and formatting"
git push"#),
        );

    let events = Event::default()
        .push(Push::default().add_branch("main"))
        .pull_request(
            PullRequest::default()
                .add_type(PullRequestType::Opened)
                .add_type(PullRequestType::Synchronize)
                .add_type(PullRequestType::Reopened)
                .add_branch("main"),
        );

    let workflow = Workflow::default()
        .name("autofix.ci")
        .add_env(RustFlags::deny("warnings"))
        .on(events)
        .concurrency(
            Concurrency::default()
                .group("autofix-${{github.ref}}")
                .cancel_in_progress(false),
        )
        .add_job("lint", lint_fix_job);

    Generate::new(workflow)
        .name("autofix.yml")
        .generate()
        .unwrap();
}
