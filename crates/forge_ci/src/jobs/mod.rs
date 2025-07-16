//! Jobs for CI workflows

mod draft_release_update_job;
mod label_sync_job;
mod release_build_job;
mod release_draft;
mod release_homebrew;
mod release_npm;

pub use draft_release_update_job::*;
pub use label_sync_job::*;
pub use release_build_job::*;
pub use release_draft::*;
pub use release_homebrew::*;
pub use release_npm::*;

/// Helper function to generate an apt-get install command for multiple packages
fn apt_get_install(packages: &[&str]) -> String {
    format!(
        "sudo apt-get update && \\\nsudo apt-get install -y \\\n{}",
        packages
            .iter()
            .map(|pkg| format!("  {pkg}"))
            .collect::<Vec<_>>()
            .join(" \\\n")
    )
}

#[cfg(test)]
mod test {
    use crate::jobs::apt_get_install;

    #[test]
    fn test_apt_get_install() {
        let packages = &["pkg1", "pkg2", "pkg3"];
        let command = apt_get_install(packages);
        assert_eq!(
            command,
            "sudo apt-get update && \\\nsudo apt-get install -y \\\n  pkg1 \\\n  pkg2 \\\n  pkg3"
        );
    }
}
