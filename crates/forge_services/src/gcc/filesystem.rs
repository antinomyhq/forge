use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use forge_domain::{ContextLevel, GccError, GccResult};

/// Initialize a GCC repository at the given path. Creates `.GCC` directory and
/// main.md file.
pub fn init_repository(base_path: &Path) -> GccResult<()> {
    let gcc_dir = base_path.join(".GCC");
    if !gcc_dir.exists() {
        fs::create_dir_all(&gcc_dir).map_err(GccError::Io)?;
    }
    // create main.md if not exists
    let main_md = gcc_dir.join("main.md");
    if !main_md.exists() {
        let mut file = File::create(&main_md).map_err(GccError::Io)?;
        file.write_all(b"# GCC Project Overview\n\n")
            .map_err(GccError::Io)?;
    }
    Ok(())
}

/// Create a new branch directory under `.GCC/branches`.
pub fn create_branch(base_path: &Path, branch_name: &str) -> GccResult<()> {
    let branches_dir = base_path.join(".GCC/branches");
    fs::create_dir_all(&branches_dir).map_err(GccError::Io)?;
    let branch_path = branches_dir.join(branch_name);
    if branch_path.exists() {
        return Err(GccError::InvalidOperation(format!(
            "Branch '{branch_name}' already exists"
        )));
    }
    fs::create_dir(&branch_path).map_err(GccError::Io)?;
    // create log.md
    let log_path = branch_path.join("log.md");
    let mut file = File::create(&log_path).map_err(GccError::Io)?;
    file.write_all(format!("# Log for branch {branch_name}\n\n").as_bytes())
        .map_err(GccError::Io)?;
    Ok(())
}

/// Write a commit file under a branch.
pub fn write_commit(
    base_path: &Path,
    branch: &str,
    commit_id: &str,
    content: &str,
) -> GccResult<()> {
    let branch_path = base_path.join(".GCC/branches").join(branch);
    if !branch_path.exists() {
        return Err(GccError::BranchNotFound(branch.to_string()));
    }
    let commit_path = branch_path.join(format!("{commit_id}.md"));
    let mut file = File::create(&commit_path).map_err(GccError::Io)?;
    file.write_all(content.as_bytes()).map_err(GccError::Io)?;
    Ok(())
}

/// Retrieve path to a context level.
pub fn context_path(base_path: &Path, level: &ContextLevel) -> PathBuf {
    match level {
        ContextLevel::Project => base_path.join(".GCC/main.md"),
        ContextLevel::Branch(name) => base_path.join(".GCC/branches").join(name).join("log.md"),
        ContextLevel::Commit(id) => {
            // Assuming id includes branch prefix like "branch/commit"
            let parts: Vec<&str> = id.split('/').collect();
            if parts.len() == 2 {
                base_path
                    .join(".GCC/branches")
                    .join(parts[0])
                    .join(format!("{}.md", parts[1]))
            } else {
                base_path.join(".GCC/main.md")
            }
        }
    }
}
