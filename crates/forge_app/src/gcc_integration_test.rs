#[cfg(test)]
mod gcc_integration_tests {
    use std::path::Path;

    use forge_domain::GccSystemContext;
    use pretty_assertions::assert_eq;

    // Test the GCC context creation logic directly
    async fn create_gcc_context_for_test(base_path: &Path) -> Option<GccSystemContext> {
        // This is the same logic as in the orchestrator
        let gcc_dir = base_path.join(".GCC");
        if !gcc_dir.exists() {
            return None;
        }

        let mut context = GccSystemContext {
            is_initialized: true,
            current_branch: None,
            available_branches: vec![],
            latest_commit: None,
            project_context: None,
            branch_context: None,
            commit_context: None,
        };

        // Get available branches
        let branches_dir = gcc_dir.join("branches");
        if branches_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&branches_dir) {
                let mut branches = Vec::new();
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            branches.push(name.to_string());
                        }
                    }
                }
                context.available_branches = branches.clone();

                // Use "main" as default branch if it exists, otherwise use first branch
                if let Some(current_branch) = branches
                    .iter()
                    .find(|&b| b == "main")
                    .or_else(|| branches.first())
                {
                    context.current_branch = Some(current_branch.clone());

                    // Get latest commit for the current branch
                    let branch_path = branches_dir.join(current_branch);
                    if branch_path.exists() {
                        if let Ok(entries) = std::fs::read_dir(&branch_path) {
                            let mut commits = Vec::new();
                            for entry in entries.flatten() {
                                if entry.path().is_file() {
                                    if let Some(ext) = entry.path().extension() {
                                        if ext == "md" {
                                            if let Some(stem) = entry.path().file_stem() {
                                                if let Some(id) = stem.to_str() {
                                                    // Skip the context.md file
                                                    if id != "context" {
                                                        commits.push(id.to_string());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if !commits.is_empty() {
                                commits.sort();
                                if let Some(latest) = commits.last() {
                                    context.latest_commit = Some(latest.clone());
                                }
                            }
                        }
                    }

                    // Read context at different levels
                    let project_file = gcc_dir.join("project.md");
                    if project_file.exists() {
                        if let Ok(content) = std::fs::read_to_string(&project_file) {
                            context.project_context = Some(content);
                        }
                    }

                    let branch_file = branches_dir.join(format!("{}/context.md", current_branch));
                    if branch_file.exists() {
                        if let Ok(content) = std::fs::read_to_string(&branch_file) {
                            context.branch_context = Some(content);
                        }
                    }

                    if let Some(commit_id) = &context.latest_commit {
                        let commit_file =
                            branches_dir.join(format!("{}/{}.md", current_branch, commit_id));
                        if commit_file.exists() {
                            if let Ok(content) = std::fs::read_to_string(&commit_file) {
                                context.commit_context = Some(content);
                            }
                        }
                    }
                }
            }
        }

        Some(context)
    }

    #[tokio::test]
    async fn test_gcc_context_creation_when_not_initialized() {
        let temp_dir = tempfile::tempdir().unwrap();

        let gcc_context = create_gcc_context_for_test(temp_dir.path()).await;

        assert_eq!(gcc_context, None);
    }

    #[tokio::test]
    async fn test_gcc_context_creation_when_initialized() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create GCC structure
        setup_test_gcc_structure(temp_dir.path());

        let gcc_context = create_gcc_context_for_test(temp_dir.path()).await;

        assert!(gcc_context.is_some());
        let context = gcc_context.unwrap();

        assert_eq!(context.is_initialized, true);
        assert_eq!(context.current_branch, Some("main".to_string()));
        assert_eq!(context.available_branches, vec!["main".to_string()]);
        assert_eq!(context.latest_commit, Some("c001".to_string()));
        assert!(context.project_context.is_some());
        assert!(context.branch_context.is_some());
        assert!(context.commit_context.is_some());

        // Verify content
        assert!(
            context
                .project_context
                .as_ref()
                .unwrap()
                .contains("Project Context")
        );
        assert!(
            context
                .branch_context
                .as_ref()
                .unwrap()
                .contains("Main Branch Context")
        );
        assert!(
            context
                .commit_context
                .as_ref()
                .unwrap()
                .contains("Commit c001")
        );
    }

    fn setup_test_gcc_structure(base_path: &std::path::Path) {
        let gcc_dir = base_path.join(".GCC");
        std::fs::create_dir_all(&gcc_dir).unwrap();

        // Create project file
        let project_file = gcc_dir.join("project.md");
        std::fs::write(&project_file, "# Project Context\nThis is a test project.").unwrap();

        // Create branches directory
        let branches_dir = gcc_dir.join("branches");
        std::fs::create_dir_all(&branches_dir).unwrap();

        let main_branch_dir = branches_dir.join("main");
        std::fs::create_dir_all(&main_branch_dir).unwrap();

        // Create branch context file
        let branch_context_file = main_branch_dir.join("context.md");
        std::fs::write(
            &branch_context_file,
            "# Main Branch Context\nThis is the main branch.",
        )
        .unwrap();

        // Create commit file
        let commit_file = main_branch_dir.join("c001.md");
        std::fs::write(&commit_file, "# Commit c001\nInitial commit content.").unwrap();
    }
}
