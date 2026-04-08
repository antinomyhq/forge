use gh_workflow::*;

/// Create a Docker release job that builds and pushes the workspace-server
/// image to GitHub Container Registry.
pub fn release_docker_job() -> Job {
    Job::new("docker-release")
        .runs_on("ubuntu-latest")
        .add_needs("build_release")
        .permissions(
            Permissions::default()
                .contents(Level::Read)
                .packages(Level::Write),
        )
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
        .add_step(
            Step::new("Set up Docker Buildx").uses("docker", "setup-buildx-action", "v3"),
        )
        .add_step(
            Step::new("Login to GitHub Container Registry")
                .uses("docker", "login-action", "v3")
                .add_with(("registry", "ghcr.io"))
                .add_with(("username", "${{ github.repository_owner }}"))
                .add_with(("password", "${{ secrets.GITHUB_TOKEN }}")),
        )
        .add_step(
            Step::new("Extract version and repository name")
                .run(
                    r#"echo "tag=${GITHUB_REF_NAME#v}" >> $GITHUB_OUTPUT
echo "repo=${GITHUB_REPOSITORY,,}" >> $GITHUB_OUTPUT"#,
                )
                .id("version"),
        )
        .add_step(
            Step::new("Build and push Docker image")
                .uses("docker", "build-push-action", "v6")
                .add_with(("context", "./server"))
                .add_with(("push", "true"))
                .add_with(("platforms", "linux/amd64"))
                .add_with(("tags", "ghcr.io/${{ steps.version.outputs.repo }}/workspace-server:${{ steps.version.outputs.tag }}\nghcr.io/${{ steps.version.outputs.repo }}/workspace-server:latest"))
                .add_with(("cache-from", "type=gha"))
                .add_with(("cache-to", "type=gha,mode=max")),
        )
}
