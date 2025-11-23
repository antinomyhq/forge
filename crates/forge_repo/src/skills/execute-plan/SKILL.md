---
name: execute-plan
description: Execute structured task plans with status tracking. Use when the user provides a plan file path in the format `plans/{current-date}-{task-name}-{version}.md` or explicitly asks you to execute a plan file.
---

# Execute Plan

Execute structured task plans with automatic status tracking and progress updates.

## Commitment to Completion

When a plan is provided, **all tasks in the plan must be completed**. Before starting execution, recite:

> "I will execute this plan to completion. All the 20 tasks will be addressed and marked as DONE."

## Execution Steps

**STEP 1**: Recite the commitment to complete all tasks in the plan.

**STEP 2**: Read the entire plan file to identify pending tasks based on `task_status`.

**STEP 3**: Announce the next pending task and update its status to `IN_PROGRESS` in the plan file.

**STEP 4**: Execute all actions required to complete the task and mark the task status to `DONE` in the plan file.

**STEP 5**: Repeat from Step 3 until all tasks are marked as `DONE`.

**STEP 6**: Re-read the plan file to verify all tasks are completed before announcing completion.

## Task Status Format

Use these status indicators in the plan file:

```
[ ]: PENDING
[~]: IN_PROGRESS
[x]: DONE
[!]: FAILED
```

## Example Usage

1. User provides: "Execute plan at plans/2025-11-23-refactor-auth-v1.md"
2. Recite commitment: "I will execute this plan to completion..."
3. Read the plan file
4. Find first `[ ]` (PENDING) task
5. Update to `[~]` (IN_PROGRESS)
6. Execute the task
7. Update to `[x]` (DONE)
8. Move to next PENDING task
9. Repeat until all tasks appear DONE
10. Re-read plan file to verify completion
11. Announce completion
