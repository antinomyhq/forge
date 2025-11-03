use std::path::PathBuf;

use forge_domain::{ActivePlan, PlanStat};

/// Parse a plan file and extract task statistics
pub fn parse_plan(path: PathBuf, content: &str) -> ActivePlan {
    let plan_stats = parse_task_stats(content);
    ActivePlan { path, stat: plan_stats }
}

/// Parse task statistics from plan content
fn parse_task_stats(content: &str) -> PlanStat {
    let mut completed = 0;
    let mut todo = 0;
    let mut failed = 0;
    let mut in_progress = 0;
    let mut in_task_status_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "<task_status>" {
            in_task_status_block = true;
            continue;
        }

        if trimmed == "</task_status>" {
            break;
        }

        if in_task_status_block && !trimmed.is_empty() {
            if trimmed.starts_with("[ ]:") {
                todo += 1;
            } else if trimmed.starts_with("[~]:") {
                in_progress += 1;
            } else if trimmed.starts_with("[x]:") {
                completed += 1;
            } else if trimmed.starts_with("[!]:") {
                failed += 1;
            }
        }
    }

    PlanStat { completed, todo, failed, in_progress }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parse_plan_with_all_statuses() {
        let content = r#"# Test Plan

<task_status>
[ ]: PENDING - Task 1
[~]: IN_PROGRESS - Task 2
[x]: DONE - Task 3
[!]: FAILED - Task 4
</task_status>
"#;
        let plan = parse_plan(PathBuf::from("/test/plan.md"), content);

        assert_eq!(plan.stat.todo, 1);
        assert_eq!(plan.stat.in_progress, 1);
        assert_eq!(plan.stat.completed, 1);
        assert_eq!(plan.stat.failed, 1);
    }

    #[test]
    fn test_parse_plan_empty() {
        let content = "# Plan\n\nNo tasks";
        let plan = parse_plan(PathBuf::from("/test/plan.md"), content);

        assert_eq!(plan.stat.todo, 0);
        assert_eq!(plan.stat.in_progress, 0);
        assert_eq!(plan.stat.completed, 0);
        assert_eq!(plan.stat.failed, 0);
    }

    #[test]
    fn test_parse_plan_all_completed() {
        let content = r#"
<task_status>
[x]: Task 1
[x]: Task 2
[x]: Task 3
</task_status>
"#;
        let plan = parse_plan(PathBuf::from("/test/plan.md"), content);

        assert_eq!(plan.stat.completed, 3);
        assert_eq!(plan.stat.todo, 0);
        assert!(plan.is_complete());
    }

    #[test]
    fn test_parse_plan_ignores_content_outside_task_status() {
        let content = r#"
[ ]: This should be ignored
<task_status>
[x]: Task 1
[ ]: Task 2
</task_status>
[x]: This should also be ignored
"#;
        let plan = parse_plan(PathBuf::from("/test/plan.md"), content);

        assert_eq!(plan.stat.completed, 1);
        assert_eq!(plan.stat.todo, 1);
    }
}
