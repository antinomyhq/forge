use generate::Generate;
use gh_workflow_tailcall::*;

use crate::jobs;

/// Helper function to add certificate creation step to a job
fn add_certificate_step_to_job(mut job: Job) -> Job {
    if let Some(steps) = &mut job.steps {
        // Find the checkout step and add certificate step after it
        for (i, step) in steps.iter().enumerate() {
            if let Some(uses) = &step.uses {
                if uses.contains("checkout") {
                    let cert_step = Step::run("echo \"$MTLS_CERT\" > cert.pem")
                        .name("Create mTLS certificate file")
                        .if_condition(Expression::new("env.MTLS_CERT != ''"));
                    steps.insert(i + 1, cert_step.into());
                    break;
                }
            }
        }
    }
    job
}

/// Generate the main CI workflow
pub fn generate_ci_workflow() {
    let workflow = StandardWorkflow::default()
        .auto_fix(true)
        .to_ci_workflow()
        .concurrency(Concurrency {
            group: "${{ github.workflow }}-${{ github.ref }}".to_string(),
            cancel_in_progress: None,
            limit: None,
        })
        .add_env(("OPENROUTER_API_KEY", "${{secrets.OPENROUTER_API_KEY}}"))
        .add_env(("MTLS_CERT", "${{secrets.MTLS_CERT}}"));

    // Get the jobs and add certificate creation step to build and lint jobs
    let build_job = if let Some(jobs) = &workflow.jobs {
        if let Some(original_build_job) = jobs.get("build") {
            add_certificate_step_to_job(original_build_job.clone())
        } else {
            jobs.get("build").unwrap().clone()
        }
    } else {
        panic!("No jobs found in workflow")
    };

    let lint_job = if let Some(jobs) = &workflow.jobs {
        if let Some(original_lint_job) = jobs.get("lint") {
            add_certificate_step_to_job(original_lint_job.clone())
        } else {
            jobs.get("lint").unwrap().clone()
        }
    } else {
        panic!("No jobs found in workflow")
    };

    let draft_release_job = jobs::create_draft_release_job(&build_job);

    // Add jobs to the workflow (this will override the original build and lint
    // jobs)
    workflow
        .add_job("build", build_job)
        .add_job("lint", lint_job)
        .add_job("draft_release", draft_release_job.clone())
        .add_job(
            "build_release",
            jobs::create_build_release_main_job(&draft_release_job),
        )
        .add_job(
            "build_release_pr",
            jobs::create_build_release_pr_job(&draft_release_job),
        )
        .generate()
        .unwrap();
}

/// Generate homebrew release workflow
pub fn generate_homebrew_workflow() {
    let homebrew_workflow = jobs::create_homebrew_workflow();

    Generate::new(homebrew_workflow)
        .name("release-homebrew.yml")
        .generate()
        .unwrap();
}

/// Generate npm release workflow
pub fn generate_npm_workflow() {
    let npm_workflow = jobs::create_npm_workflow();

    Generate::new(npm_workflow)
        .name("release-npm.yml")
        .generate()
        .unwrap();
}

/// Generate release drafter workflow
pub fn generate_release_drafter_workflow() {
    let release_drafter_workflow = jobs::create_release_drafter_workflow();

    Generate::new(release_drafter_workflow)
        .name("release-drafter.yml")
        .generate()
        .unwrap();
}

pub fn generate_labels_workflow() {
    let labels_workflow = jobs::create_labels_workflow();

    Generate::new(labels_workflow)
        .name("labels.yml")
        .generate()
        .unwrap();
}
